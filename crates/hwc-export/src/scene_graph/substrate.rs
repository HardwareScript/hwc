//! Substrate layer processing and net-aware clustering

use super::mesh_generation::{create_box_with_holes_mesh, create_via_mesh, ViaMeshParams};
use super::types::{BoxParams, MaterialNode, MeshNode};
use crate::geometry_union::{circle_to_path, rect_to_path, stroke_route_segments};
use crate::mesh_extrusion::extrude_polygon_mesh;
use clipper2_rust::core::FillRule;
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

    // --- PURE VECTOR PATH OFFSETTING: The Font-Engine Exporter ---
    // Generate meshes directly from topological router output using Clipper2's native
    // path stroking engine. This preserves pristine 45° miters and arbitrary angles.

    let mut analytic_copper_pools: FxHashMap<
        (
            i64,
            i64,
            hwc_engine::geometry_router::substrate_types::MaterialId,
            u32,
        ),
        Vec<clipper2_rust::Path64>,
    > = FxHashMap::default();

    // Add substrate layer pads (Pad_A, Pad_B, obstacles, etc.) to the pools
    use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;

    for layer in substrate_layers {
        if layer.layer_type == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour {
            let key = (
                layer.bbox.min.z,
                layer.bbox.max.z,
                layer.material,
                layer.net,
            );

            let path = match layer.shape {
                SubstrateLayerShape::Rect => rect_to_path(&layer.bbox),
                SubstrateLayerShape::Circle { radius } => {
                    let cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                    let cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                    circle_to_path(cx, cy, radius, 64)
                }
                _ => continue,
            };

            analytic_copper_pools.entry(key).or_default().push(path);
        }
    }

    // Gather trace paths from analytic routes using native path offsetting
    for route in &space.analytic_routes {
        let half_t = route.cross_section.thickness_nm / 2;

        let z_min = route
            .segments
            .iter()
            .map(|s| s.start.z.min(s.end.z))
            .min()
            .unwrap_or(0)
            - half_t;
        let z_max = route
            .segments
            .iter()
            .map(|s| s.start.z.max(s.end.z))
            .max()
            .unwrap_or(0)
            + half_t;

        // Use the shared stroke_route_segments function to generate perfect mitered outlines
        let trace_outline = stroke_route_segments(&route.segments, route.cross_section.width_nm);

        let key = (z_min, z_max, route.material, route.net_id.raw());
        analytic_copper_pools
            .entry(key)
            .or_default()
            .extend(trace_outline);
    }

    // Add via pads to analytic copper pools
    for via in &space.vias {
        let z_start = via.from_z_nm.min(via.to_z_nm);
        let z_end = via.from_z_nm.max(via.to_z_nm);
        let pad_radius = via.diameter_nm / 2 + via.annular_ring_nm.max(via.diameter_nm / 4);
        let copper_thickness = 35_000;

        let copper_material_id = space
            .material_registry
            .all_materials()
            .into_iter()
            .find(|(_, name)| {
                name.contains("Copper") || name.contains("Aluminum") || name.contains("Metal")
            })
            .map(|(id, _)| id)
            .unwrap_or(space.substrate_material_id);

        // Top pad
        let top_key = (
            z_end - copper_thickness,
            z_end,
            copper_material_id,
            via.net_id.raw(),
        );
        analytic_copper_pools
            .entry(top_key)
            .or_default()
            .push(circle_to_path(
                via.position.0,
                via.position.1,
                pad_radius,
                64,
            ));

        // Bottom pad
        let bottom_key = (
            z_start,
            z_start + copper_thickness,
            copper_material_id,
            via.net_id.raw(),
        );
        analytic_copper_pools
            .entry(bottom_key)
            .or_default()
            .push(circle_to_path(
                via.position.0,
                via.position.1,
                pad_radius,
                64,
            ));
    }

    // Union and extrude analytic trace segments with proper welding
    for ((z_min_nm, z_max_nm, material_id, net_raw), paths) in analytic_copper_pools {
        let material_name = space
            .material_registry
            .get_name(material_id)
            .unwrap_or("Copper");

        // Perform 2D Boolean Union to weld overlapping rectangles
        let unioned = clipper2_rust::union_64(&paths, &vec![], FillRule::NonZero);

        let z_min_mm = z_min_nm as f64 / 1_000_000.0;
        let depth_mm = (z_max_nm - z_min_nm) as f64 / 1_000_000.0;

        for (c_idx, path) in unioned.iter().enumerate() {
            if path.len() >= 3 {
                let outer_points: Vec<(f64, f64)> = path
                    .iter()
                    .map(|pt| (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0))
                    .collect();

                meshes.push(extrude_polygon_mesh(
                    &format!("Analytic_Route_Net_{}_Contour_{}", net_raw, c_idx),
                    &outer_points,
                    &[],
                    z_min_mm,
                    depth_mm,
                    material_name,
                    space.view,
                ));
            }
        }
    }

    // Render via barrel tubes
    for (via_idx, via) in space.vias.iter().enumerate() {
        let z_start = via.from_z_nm.min(via.to_z_nm);
        let z_end = via.from_z_nm.max(via.to_z_nm);

        let z_min_mm = z_start as f64 / 1_000_000.0;
        let depth_mm = (z_end - z_start) as f64 / 1_000_000.0;
        let center_x_mm = via.position.0 as f64 / 1_000_000.0;
        let center_y_mm = via.position.1 as f64 / 1_000_000.0;
        let outer_dia_mm = via.diameter_nm as f64 / 1_000_000.0;
        let pad_dia_mm = (via.diameter_nm + 2 * via.annular_ring_nm.max(via.diameter_nm / 4))
            as f64
            / 1_000_000.0;
        let barrel_thickness_mm = outer_dia_mm;

        let material_name = space
            .material_registry
            .get_name(space.substrate_material_id)
            .unwrap_or("Copper");

        meshes.push(create_via_mesh(ViaMeshParams {
            name: format!("Analytic_Via_Barrel_{}", via_idx),
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

    // Legacy substrate layer system removed - using pure analytic routes only

    // Render substrate base (FR4) if present
    for (idx, layer) in substrate_layers.iter().enumerate() {
        if layer.layer_type
            != hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Substrate
        {
            continue;
        }

        let material_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");

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
