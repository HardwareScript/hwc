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

/// Infer material properties from name patterns using heuristics.
/// This provides scalable fallback for user-defined materials without hardcoded names.
pub fn infer_material_from_name(name: &str) -> MaterialNode {
    let name_lower = name.to_lowercase();

    // Pattern-based inference for precedence and visual properties
    let (color, precedence, metallic, roughness) =
        if name_lower.contains("copper") || name_lower.contains("cu") {
            (Color::new(184, 115, 51), 1, 1.0, 0.2)
        } else if name_lower.contains("gold") || name_lower.contains("au") {
            (Color::new(255, 215, 0), 1, 1.0, 0.1)
        } else if name_lower.contains("silver") || name_lower.contains("ag") {
            (Color::new(192, 192, 192), 1, 1.0, 0.1)
        } else if name_lower.contains("aluminum")
            || name_lower.contains("al")
            || name_lower.contains("alu")
        {
            (Color::new(200, 200, 200), 1, 0.9, 0.3)
        } else if name_lower.contains("pour")
            || name_lower.contains("fill")
            || name_lower.contains("thief")
        {
            // Dummy metal fill (thieving) - typically copper-like
            (Color::new(184, 115, 51), 4, 1.0, 0.2)
        } else if name_lower.contains("silicon") || name_lower.contains("si") {
            (Color::new(100, 150, 100), 2, 0.1, 0.7)
        } else if name_lower.contains("soldermask") || name_lower.contains("mask") {
            let shade = if name_lower.contains("green") {
                Color::new(0, 150, 0)
            } else if name_lower.contains("red") {
                Color::new(200, 0, 0)
            } else if name_lower.contains("blue") {
                Color::new(0, 0, 200)
            } else {
                Color::new(0, 100, 0)
            };
            (shade, 3, 0.0, 0.5)
        } else if name_lower.contains("solder") {
            (Color::new(200, 200, 200), 1, 0.2, 0.8)
        } else if name_lower.contains("substrate")
            || name_lower.contains("fr4")
            || name_lower.contains("pcb")
        {
            (Color::new(26, 77, 26), 4, 0.0, 0.8)
        } else if name_lower.contains("via") || name_lower.contains("trace") {
            (Color::new(184, 115, 51), 1, 1.0, 0.2)
        } else if name_lower.contains("component") {
            (Color::new(40, 40, 40), 2, 0.1, 0.5)
        } else if name_lower.contains("dielectric") || name_lower.contains("prepreg") {
            (Color::new(100, 100, 150), 4, 0.0, 0.6)
        } else {
            // Default fallback: assume conductor-like for unknown materials
            (Color::new(184, 115, 51), 4, 1.0, 0.2)
        };

    MaterialNode {
        name: name.into(),
        color,
        opacity: 1.0,
        outline_opacity: 1.0,
        metallic,
        roughness,
        ior: 1.5,
        clearcoat: 0.0,
        clearcoat_roughness: 0.0,
        subsurface: 0.0,
        anisotropy: 0.0,
        anisotropy_rotation: 0.0,
        texture: None,
        precedence,
    }
}

/// Get or create a material with depth bias metadata for the glTF export.
/// Uses a lookup table approach with pattern-based inference for unknown materials.
pub fn get_or_create_material<'a>(
    materials: &'a mut FxHashMap<CompactString, MaterialNode>,
    name: &CompactString,
) -> (&'a MaterialNode, CompactString) {
    let is_new = !materials.contains_key(name);
    if is_new {
        let material_node = infer_material_from_name(name.as_str());
        materials.insert(name.clone(), material_node);
    }
    let mat = materials.get(name).unwrap();
    (mat, name.clone())
}

/// Add materials from Symbol Table
pub fn add_materials_from_symbol_table(
    materials: &mut FxHashMap<CompactString, MaterialNode>,
    symbol_table: &SymbolTable,
) -> Result<(), SceneGraphError> {
    for (name, material_def) in symbol_table.materials() {
        // Extract color (with fallback to material's get_color method)
        let color_hex = material_def.get_color();
        let color = parse_hex_color(&color_hex)?;

        // Extract visual properties with defaults (v0.1.6 God-Tier Visual API)
        let mut opacity = material_def.get_opacity() as f32;

        // v0.1.7: Force components and semiconductor bodies to be Opaque
        if material_def.category == hwc_parser::MaterialCategory::Semiconductor
            || name.to_lowercase().contains("body")
            || name.to_lowercase().contains("component")
        {
            opacity = 1.0;
        }

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
        let precedence = match material_def.category {
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

    // Standard Procedural Materials (Limitation 6)
    let default_materials = [
        ("Copper", Color::new(184, 115, 51), 1.0, 1.0, 1.0, 0.2), // #B87333
        ("FR4", Color::new(26, 77, 26), 1.0, 0.1, 0.0, 0.8),      // Dark Green
        ("SolderMask", Color::new(0, 100, 0), 0.8, 0.0, 0.0, 0.5),
        ("SilkScreen", Color::new(240, 240, 240), 1.0, 0.0, 0.0, 1.0),
        ("Component", Color::new(26, 26, 26), 1.0, 1.0, 0.0, 0.5), // #1A1A1A
        ("Gold", Color::new(255, 215, 0), 1.0, 1.0, 1.0, 0.1),
        ("Silver", Color::new(192, 192, 192), 1.0, 1.0, 1.0, 0.1),
    ];

    for (name, color, opacity, outline_opacity, metallic, roughness) in default_materials {
        materials
            .entry(CompactString::from(name))
            .or_insert_with(|| MaterialNode {
                name: name.into(),
                color,
                opacity,
                outline_opacity,
                metallic,
                roughness,
                ior: 1.5,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                subsurface: 0.0,
                anisotropy: 0.0,
                anisotropy_rotation: 0.0,
                texture: None,
                precedence: match name {
                    "Copper" | "Gold" | "Silver" => 1,
                    "Component" => 2,
                    "SolderMask" | "SilkScreen" => 3,
                    _ => 4,
                },
            });
    }

    Ok(())
}
