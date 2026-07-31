//! Substrate layer processing and net-aware clustering
//!
//! **v0.2.2**: This module now uses the unified_geometry module as the single source
//! of truth for copper contours. It focuses on 3D mesh extrusion and substrate base
//! rendering, delegating all 2D geometry calculations to unified_geometry.

use super::mesh_generation::{create_box_with_holes_mesh, create_via_mesh, ViaMeshParams};
use super::types::{BoxParams, MaterialNode, MeshNode};
use crate::mesh_extrusion::extrude_polygon_mesh;
use compact_str::CompactString;
use hwc_engine::geometry_router::entity_graph::CapType;
use hwc_engine::HardwareSpace;
use hwc_parser::ProfileDefinition;
use rustc_hash::FxHashMap;

/// Add substrate mesh (FR4 base) from analytic routes
pub fn add_substrate(
    meshes: &mut Vec<MeshNode>,
    space: &HardwareSpace,
    _materials: &FxHashMap<CompactString, MaterialNode>,
    _profile: Option<&ProfileDefinition>,
) {
    let substrate_layers = space.entity_graph.get_substrate_layers();

    // **v0.2.2: USE UNIFIED GEOMETRY (SINGLE SOURCE OF TRUTH)**
    // All copper contours come from unified_geometry module.
    // This module only handles 3D extrusion of those contours.
    let copper_contours = crate::scene_graph::generate_copper_contours(space);
    
    eprintln!("[SUBSTRATE MESH] Processing {} unified copper pools", copper_contours.len());
    
    // Extrude each unified copper contour into 3D meshes
    for contour_data in copper_contours {
        let z_min_mm = contour_data.key.z_min as f64 / 1_000_000.0;
        let depth_mm = (contour_data.key.z_max - contour_data.key.z_min) as f64 / 1_000_000.0;
        
        let material_name = space
            .material_registry
            .get_name(contour_data.key.material)
            .expect(&format!(
                "Material ID {:?} not found in registry for substrate mesh",
                contour_data.key.material
            ));

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
                &format!("Copper_Net{}_Contour{}", contour_data.key.net_id.raw(), c_idx),
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
            .expect(&format!("Material ID {:?} not found in registry for via {}", via.material_id, via_idx));
        
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

    // **SUBSTRATE BASE**: Render the FR4 or silicon substrate base
    for (idx, layer) in substrate_layers.iter().enumerate() {
        if layer.layer_type
            != hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Substrate
        {
            continue;
        }

        let material_name = space
            .material_registry
            .get_name(layer.material)
            .expect(&format!("Material ID {:?} not found in registry for substrate base layer {}", layer.material, idx));

        if material_name == "Void" || material_name == "Air" {
            continue;
        }

        let min_x_mm = layer.bbox.min.x as f64 / 1_000_000.0;
        let min_y_mm = layer.bbox.min.y as f64 / 1_000_000.0;
        let min_z_mm = layer.bbox.min.z as f64 / 1_000_000.0;
        let max_x_mm = layer.bbox.max.x as f64 / 1_000_000.0;
        let max_y_mm = layer.bbox.max.y as f64 / 1_000_000.0;
        let max_z_mm = layer.bbox.max.z as f64 / 1_000_000.0;
        let width = max_x_mm - min_x_mm;
        let height = max_y_mm - min_y_mm;
        let depth = max_z_mm - min_z_mm;

        meshes.push(MeshNode {
            name: format!("Substrate_Base_{}", idx).into(),
            vertices: Vec::new(),
            faces: create_box_with_holes_mesh(
                &format!("Substrate_Base_{}", idx),
                BoxParams {
                    x: min_x_mm,
                    y: min_y_mm,
                    z: min_z_mm,
                    width,
                    height,
                    depth,
                },
                vec![],
                material_name,
                space.view,
                super::types::FaceCulling::none(),
            )
            .faces,
            material_name: material_name.to_string().into(),
        });
    }
}
