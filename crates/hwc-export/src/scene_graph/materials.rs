//! Material handling and color parsing

use super::types::{Color, MaterialNode};
use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use rustc_hash::FxHashMap;

/// Scene graph errors
#[derive(Debug, thiserror::Error)]
pub enum SceneGraphError {
    #[error("Invalid color format: {0}")]
    InvalidColor(String),

    #[error("Failed to parse hex color: {0}")]
    ParseError(#[from] std::num::ParseIntError),

    #[error("Component definition not found: {component}")]
    ComponentNotFound { component: String },

    #[error(
        "Material '{material}' is not declared in the PDK. \
         Add a material definition to your materials.hw file or PDK profile."
    )]
    MaterialNotFound { material: String },
}

/// Parse hex color string (#RRGGBB)
pub fn parse_hex_color(hex: &str) -> Result<Color, SceneGraphError> {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Err(SceneGraphError::InvalidColor(hex.into()));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;

    Ok(Color::new(r, g, b))
}

/// Look up an existing material by name. Fails if the material was not declared in the PDK.
///
/// No inference or heuristics — if a material is not declared in the symbol table,
/// this returns an error so the compiler fails loudly.
pub fn get_or_create_material<'a>(
    materials: &'a mut FxHashMap<CompactString, MaterialNode>,
    name: &CompactString,
) -> Result<(&'a MaterialNode, CompactString), SceneGraphError> {
    if !materials.contains_key(name) {
        return Err(SceneGraphError::MaterialNotFound {
            material: name.to_string(),
        });
    }
    let mat = materials.get(name).unwrap();
    Ok((mat, name.clone()))
}

/// Add materials from Symbol Table
pub fn add_materials_from_symbol_table(
    materials: &mut FxHashMap<CompactString, MaterialNode>,
    symbol_table: &SymbolTable,
) -> Result<(), SceneGraphError> {
    for (name, material_def) in symbol_table.materials() {
        // Extract color (with fallback to material's get_color method)
        let color_hex = material_def.get_color().unwrap_or_else(|| "#808080".into());
        let color = parse_hex_color(&color_hex)?;

        // Extract visual properties with defaults (v0.1.6 God-Tier Visual API)
        let mut opacity = material_def.get_opacity() as f32;

        eprintln!(
            "[MATERIAL DEBUG] Material '{}': category={:?}, initial_opacity={}",
            name, material_def.category(), opacity
        );

        if name.to_lowercase().contains("body") || name.to_lowercase().contains("component") {
            eprintln!(
                "[MATERIAL DEBUG] Material '{}': Forcing opacity to 1.0 (body/component)",
                name
            );
            opacity = 1.0;
        }

        eprintln!(
            "[MATERIAL DEBUG] Material '{}': final_opacity={}",
            name, opacity
        );

        let outline_opacity = material_def.get_outline_opacity() as f32;
        let roughness = material_def.get_roughness() as f32;
        let metallic = material_def.get_metallic() as f32;
        let ior = material_def.get_ior() as f32;
        let clearcoat = material_def.get_clearcoat() as f32;
        let clearcoat_roughness = material_def.get_clearcoat_roughness() as f32;
        let subsurface = material_def.get_subsurface() as f32;
        let anisotropy = material_def.get_anisotropy() as f32;
        let anisotropy_rotation = material_def.get_anisotropy_rotation() as f32;
        let texture = material_def.get_texture();

        // v0.1.7 Manifold Export: Precedence calculation
        // Level 1: Metals/Conductors
        // Level 2: Semiconductors
        // Level 3: Protective Layers (Solder Mask)
        // Level 4: Substrates
        let precedence = match material_def.category() {
            hwc_parser::MaterialCategory::Conductor
            | hwc_parser::MaterialCategory::OhmicContact
            | hwc_parser::MaterialCategory::DieInterconnect
            | hwc_parser::MaterialCategory::PcbSolder
            | hwc_parser::MaterialCategory::BarrierLayer
            | hwc_parser::MaterialCategory::Adhesive => 1,
            hwc_parser::MaterialCategory::Semiconductor => 2,
            hwc_parser::MaterialCategory::Insulator => {
                // Heuristic: If material name contains "SolderMask", it's level 3
                if name.to_lowercase().contains("soldermask") {
                    3
                } else {
                    4
                }
            }
            hwc_parser::MaterialCategory::Mask => 3,
        };

        materials.insert(
            name.clone(),
            MaterialNode {
                name: name.clone(),
                color,
                opacity,
                outline_opacity,
                metallic,
                roughness,
                ior,
                clearcoat,
                clearcoat_roughness,
                subsurface,
                anisotropy,
                anisotropy_rotation,
                texture,
                precedence,
            },
        );
    }

    // Add special "Void" material for substrate cutouts
    // This represents negative space (mounting holes, edge cuts, etc.)
    materials
        .entry("Void".into())
        .or_insert_with(|| MaterialNode {
            name: "Void".into(),
            color: Color::new(0, 0, 0), // Black (will be rendered as transparent)
            opacity: 0.0,
            outline_opacity: 0.0,
            metallic: 0.0,
            roughness: 1.0,
            ior: 1.5,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            subsurface: 0.0,
            anisotropy: 0.0,
            anisotropy_rotation: 0.0,
            texture: None,
            precedence: 0, // Void has absolute highest precedence (it overrides everything)
        });

    Ok(())
}
