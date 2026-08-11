//! Substrate layer processing and net-aware clustering
//!
//! **v0.2.2 Industry Standard 3D CAD Architecture**
//!
//! This module implements the physical truth of semiconductor/PCB manufacturing:
//!
//! ## Copper Geometry (From unified_geometry module)
//! - All conductive pads and traces are SOLID (no holes punched!)
//! - Vias and pads on the same net are Boolean-unioned into single solid shapes
//! - unified_geometry module is the single source of truth for 2D contours
//!
//! ## Substrate Base (Rendered here)
//! - Dielectric layers (Silicon_Dioxide, FR4) get via hole cutouts
//! - SubstrateMeshBuilder performs Clipper2 Boolean DIFFERENCE to cut holes
//! - Via pillars are rendered as solid cylinders/prisms inside these holes
//!
//! ## Physical Stack
//! ```text
//! Top Conductive Pad (SOLID) ← No hole!
//!     ↓ (via pillar passes through pad, both solid, unioned)
//! Dielectric Layer (HOLE CUT) ← Only layer with hole!
//!     ↓ (via pillar fills the hole)
//! Bottom Conductive Pad (SOLID) ← No hole!
//! ```

use super::mesh_generation::{create_via_mesh, ViaMeshParams};
use super::types::{MaterialNode, MeshNode};
use crate::mesh_extrusion::extrude_polygon_mesh;
use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::geometry_router::entity_graph::CapType;
use hwc_engine::HardwareSpace;
use hwc_parser::ProfileDefinition;
use rustc_hash::FxHashMap;

/// Returns true if the named material is a zero-thickness mask (v0.2.1).
///
/// Mask materials are 2D fabrication instructions and must never produce a 3D mesh.
fn is_mask_material(symbol_table: &SymbolTable, material_name: &str) -> bool {
    symbol_table
        .get_material(material_name)
        .map(|mat_def| mat_def.category.is_zero_thickness())
        .unwrap_or(false)
}

/// Add substrate mesh (FR4 base) from analytic routes
pub fn add_substrate(
    meshes: &mut Vec<MeshNode>,
    space: &HardwareSpace,
    _materials: &FxHashMap<CompactString, MaterialNode>,
    _profile: Option<&ProfileDefinition>,
    symbol_table: &SymbolTable,
) {
    let substrate_layers = space.entity_graph.get_substrate_layers();

    // **v0.2.2: USE UNIFIED GEOMETRY (SINGLE SOURCE OF TRUTH)**
    // All copper contours come from unified_geometry module.
    // This module only handles 3D extrusion of those contours.
    let copper_contours = crate::scene_graph::generate_copper_contours(space);

    eprintln!(
        "[SUBSTRATE MESH] Processing {} unified copper pools",
        copper_contours.len()
    );

    // Extrude each unified copper contour into 3D meshes
    for contour_data in copper_contours {
        let z_min_mm = contour_data.key.z_min as f64 / 1_000_000.0;
        let depth_mm = (contour_data.key.z_max - contour_data.key.z_min) as f64 / 1_000_000.0;

        let material_name = space
            .material_registry
            .get_name(contour_data.key.material)
            .unwrap_or_else(|| {
                panic!(
                    "Material ID {:?} not found in registry for substrate mesh",
                    contour_data.key.material
                )
            });

        // v0.2.1: Skip zero-thickness masks in 3D GLB export.
        // Masks are 2D-only fabrication instructions (preserved in DXF).
        if is_mask_material(symbol_table, material_name) {
            eprintln!(
                "[SUBSTRATE MESH] Skipping mask layer '{}' (zero-thickness, 2D-only)",
                material_name
            );
            continue;
        }

        eprintln!(
            "[SUBSTRATE MESH] Extruding {} contours for net={:?}, Z={}→{}nm, material={}",
            contour_data.contours.len(),
            contour_data.key.net_id,
            contour_data.key.z_min,
            contour_data.key.z_max,
            material_name
        );

        for (c_idx, path) in contour_data.contours.iter().enumerate() {
            if path.len() < 3 {
                continue; // Skip degenerate contours
            }

            let outer_points: Vec<(f64, f64)> = path
                .iter()
                .map(|pt| (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0))
                .collect();

            meshes.push(extrude_polygon_mesh(
                &format!(
                    "Copper_Net{}_Contour{}",
                    contour_data.key.net_id.raw(),
                    c_idx
                ),
                &outer_points,
                &[],
                z_min_mm,
                depth_mm,
                material_name,
                space.view,
            ));
        }
    }

    // **VIA BARRELS**: Render cylindrical via barrels (ONLY for drilled/plated PCB vias)
    // IC vias (deposited) are already handled via Contact substrate layers (extruded above)
    for (via_idx, via) in space.vias.iter().enumerate() {
        let z_start = via.from_z_nm.min(via.to_z_nm);
        let z_end = via.from_z_nm.max(via.to_z_nm);

        let z_min_mm = z_start as f64 / 1_000_000.0;
        let depth_mm = (z_end - z_start) as f64 / 1_000_000.0;
        let center_x_mm = via.position.0 as f64 / 1_000_000.0;
        let center_y_mm = via.position.1 as f64 / 1_000_000.0;
        let outer_dia_mm = via.diameter_nm as f64 / 1_000_000.0;

        let material_name = space
            .material_registry
            .get_name(via.material_id)
            .unwrap_or_else(|| {
                panic!(
                    "Material ID {:?} not found in registry for via {}",
                    via.material_id, via_idx
                )
            });

        // v0.2.1: A zero-thickness mask can never be a physical via barrel.
        if is_mask_material(symbol_table, material_name) {
            eprintln!(
                "[SUBSTRATE MESH] Skipping mask via barrel '{}' (zero-thickness, 2D-only)",
                material_name
            );
            continue;
        }

        // Check if this is an IC via (deposited) or PCB via (drilled/plated)
        let is_ic_via = space
            .material_registry
            .get_process(via.material_id)
            .map(|process| process == hwc_engine::ManufacturingProcess::Deposited)
            .unwrap_or(false);

        if is_ic_via {
            // IC vias are fully represented by Contact substrate layers (already extruded)
            continue;
        }

        // PCB via: Render plated through-hole barrel
        let pad_dia_mm = (via.diameter_nm + 2 * via.annular_ring_nm.max(via.diameter_nm / 4))
            as f64
            / 1_000_000.0;
        let barrel_thickness_mm = outer_dia_mm;

        meshes.push(create_via_mesh(ViaMeshParams {
            name: format!("PCB_Via_Barrel_{}", via_idx),
            center: (center_x_mm, center_y_mm, z_min_mm),
            drill_dia: outer_dia_mm,
            pad_dia: pad_dia_mm,
            plating_thickness: barrel_thickness_mm,
            height: depth_mm,
            segments: 32,
            top_cap: CapType::None,
            bottom_cap: CapType::None,
            bottom_drill_dia: None,
            material_name: material_name.to_string(),
            view: space.view,
        }));
    }

    // **SUBSTRATE BASE**: Render the FR4 or silicon substrate base with via cutouts
    for (idx, layer) in substrate_layers.iter().enumerate() {
        if layer.layer_type
            != hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Substrate
        {
            continue;
        }

        let material_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or_else(|| {
                panic!(
                    "Material ID {:?} not found in registry for substrate base layer {}",
                    layer.material, idx
                )
            });

        if material_name == "Void" || material_name == "Air" {
            continue;
        }

        // v0.2.1: Zero-thickness masks never produce a 3D substrate body.
        if is_mask_material(symbol_table, material_name) {
            eprintln!(
                "[SUBSTRATE BASE] Skipping mask layer '{}' (zero-thickness, 2D-only)",
                material_name
            );
            continue;
        }

        eprintln!(
            "[SUBSTRATE BASE] Building substrate layer {} (Z={}→{}nm, material={}) with via cutouts",
            idx, layer.bbox.min.z, layer.bbox.max.z, material_name
        );

        // **v0.2.2: Use production-grade SubstrateMeshBuilder with Clipper2 + Earcut**
        // Collect all vias that should create cutouts in this substrate layer
        let via_cutouts: Vec<super::mesh_generation::ViaCutout> = space
            .vias
            .iter()
            .enumerate()
            .filter_map(|(via_idx, via)| {
                // Get the via's substrate layer representation to check its shape
                let via_substrate_layer = space
                    .entity_graph
                    .get_substrate_layers()
                    .iter()
                    .find(|layer| {
                        layer.layer_type == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Contact
                            && layer.net == via.net_id
                            && (layer.bbox.min.x + layer.bbox.max.x) / 2 == via.position.0
                            && (layer.bbox.min.y + layer.bbox.max.y) / 2 == via.position.1
                    });

                if let Some(substrate_layer) = via_substrate_layer {
                    // Check if this is a polygon via (square/rectangular IC via)
                    match &substrate_layer.shape {
                        hwc_engine::geometry_router::substrate_types::SubstrateLayerShape::Polygon { outer_contour, .. } => {
                            eprintln!(
                                "[SUBSTRATE CUTOUT] Via {} is polygonal ({} vertices), creating polygon cutout",
                                via_idx,
                                outer_contour.len()
                            );
                            Some(super::mesh_generation::ViaCutout::new_polygonal(
                                outer_contour.clone(),
                                via.from_z_nm.min(via.to_z_nm),
                                via.from_z_nm.max(via.to_z_nm),
                            ).ok()?)
                        }
                        _ => {
                            // Circular or other shape - use circular cutout
                            eprintln!(
                                "[SUBSTRATE CUTOUT] Via {} is circular (dia={}nm), creating circular cutout",
                                via_idx,
                                via.diameter_nm
                            );
                            Some(super::mesh_generation::ViaCutout::new_circular(
                                via.position.0,
                                via.position.1,
                                via.diameter_nm,
                                via.from_z_nm.min(via.to_z_nm),
                                via.from_z_nm.max(via.to_z_nm),
                            ).ok()?)
                        }
                    }
                } else {
                    // Fallback: create circular cutout
                    super::mesh_generation::ViaCutout::new_circular(
                        via.position.0,
                        via.position.1,
                        via.diameter_nm,
                        via.from_z_nm.min(via.to_z_nm),
                        via.from_z_nm.max(via.to_z_nm),
                    ).ok()
                }
            })
            .collect();

        eprintln!(
            "[SUBSTRATE BASE] Found {} valid vias for cutout consideration",
            via_cutouts.len()
        );

        // Build the substrate mesh with via cutouts using clean builder pattern
        match super::mesh_generation::SubstrateMeshBuilder::new(
            layer.bbox,
            material_name.to_string(),
            space.view,
        )
        .with_vias(via_cutouts)
        .build(&format!("Substrate_Base_{}", idx))
        {
            Ok(mesh_node) => {
                eprintln!(
                    "[SUBSTRATE BASE] Successfully generated mesh with {} vertices, {} faces",
                    mesh_node.vertices.len(),
                    mesh_node.faces.len()
                );
                meshes.push(mesh_node);
            }
            Err(e) => {
                eprintln!(
                    "[SUBSTRATE BASE ERROR] Failed to generate substrate mesh: {}",
                    e
                );
            }
        }
    }
}
