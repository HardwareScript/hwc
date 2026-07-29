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

    // KEY STRUCTURE: (z_min, z_max, material_id, net_id)
    // **TYPE-SAFE KEY**: Uses official NetId and MaterialId structs to prevent silent type mismatches
    // This enforces compile-time safety - no more u32 → NetId conversion bugs
    let mut analytic_copper_pools: FxHashMap<
        (
            i64,        // z_min
            i64,        // z_max
            hwc_engine::MaterialId, // Type-safe MaterialId (u8)
            hwc_engine::netlist::NetId, // Type-safe NetId struct
        ),
        Vec<clipper2_rust::Path64>,
    > = FxHashMap::default();

    // Add substrate layer pads (Pad_A, Pad_B, obstacles, etc.) to the pools
    use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;

    eprintln!("[MESH SUBSTRATE DEBUG] Processing {} substrate layers", substrate_layers.len());
    
    for layer in substrate_layers {
        if layer.layer_type == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour
            || layer.layer_type == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Contact
        {
            let material_name = space
                .material_registry
                .get_name(layer.material)
                .expect(&format!("Material ID {:?} not found in registry for substrate layer", layer.material));
            
            // Use the substrate layer's NetId directly (it's already strongly typed)
            let key = (
                layer.bbox.min.z,
                layer.bbox.max.z,
                layer.material,
                layer.net,
            );
            
            eprintln!(
                "[SUBSTRATE POOL KEY] net={:?} type={:?} material={} Z={}→{}nm → key=(z_min={}, z_max={}, mat={:?}, net={:?}) ADDING PATH",
                layer.net,
                layer.layer_type,
                material_name,
                layer.bbox.min.z,
                layer.bbox.max.z,
                key.0,
                key.1,
                key.2,
                key.3
            );

            let path = match &layer.shape {
                SubstrateLayerShape::Rect => rect_to_path(&layer.bbox),
                SubstrateLayerShape::Circle { radius } => {
                    let cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                    let cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                    circle_to_path(cx, cy, *radius, 64)
                }
                SubstrateLayerShape::Polygon { outer_contour, .. } => {
                    // Polygon points are now stored in world space, use directly
                    outer_contour.clone()
                }
                _ => continue,
            };

            let current_count = analytic_copper_pools.get(&key).map(|v| v.len()).unwrap_or(0);
            analytic_copper_pools.entry(key).or_default().push(path);
            eprintln!(
                "[SUBSTRATE POOL ACCUMULATE] key=({},{},{:?},{:?}) now has {} paths (was {})",
                key.0, key.1, key.2, key.3,
                analytic_copper_pools.get(&key).unwrap().len(),
                current_count
            );
        }
    }

    // Gather trace paths from analytic routes using native path offsetting
    for route in &space.analytic_routes {
        // **CRITICAL FIX**: Stroke the ENTIRE route as a continuous path, not segment-by-segment.
        // This matches the DXF exporter behavior and produces smooth, properly mitered geometry.
        // Processing segments individually causes jagged spikes when they're unioned.
        
        // Use the shared stroke_route_segments function to generate perfect mitered outlines
        // from the complete waypoint sequence
        let trace_outline = stroke_route_segments(&route.segments, route.cross_section.width_nm);
        
        let (z_min, z_max) = if let Some(range) = route.layer_z_range {
            range
        } else {
            let half_t = route.cross_section.thickness_nm / 2;
            let z_min = route
                .segments
                .iter()
                .map(|s| s.start.z.min(s.end.z))
                .min()
                .expect(&format!("Route '{}' has no segments - cannot determine Z range", route.net_name))
                - half_t;
            let z_max = route
                .segments
                .iter()
                .map(|s| s.start.z.max(s.end.z))
                .max()
                .expect(&format!("Route '{}' has no segments - cannot determine Z range", route.net_name))
                + half_t;
            (z_min, z_max)
        };
        
        let key = (z_min, z_max, route.material, route.net_id);
        
        eprintln!(
            "[TRACE POOL KEY] net={} material={} Z={}→{}nm → key=(z_min={}, z_max={}, mat={:?}, net={:?}) ADDING {} PATHS",
            route.net_name,
            space.material_registry.get_name(route.material).expect(&format!("Material ID {:?} not found in registry for route {}", route.material, route.net_name)),
            z_min,
            z_max,
            key.0,
            key.1,
            key.2,
            key.3,
            trace_outline.len()
        );
        
        let current_count = analytic_copper_pools.get(&key).map(|v| v.len()).unwrap_or(0);
        analytic_copper_pools
            .entry(key)
            .or_default()
            .extend(trace_outline);
        eprintln!(
            "[TRACE POOL ACCUMULATE] key=({},{},{:?},{:?}) now has {} paths (was {})",
            key.0, key.1, key.2, key.3,
            analytic_copper_pools.get(&key).unwrap().len(),
            current_count
        );
    }

    // Via pads are now derived natively from the substrate layer geometry (Contact type)

    // Union and extrude analytic trace segments with proper welding
    eprintln!("[MESH TRACE DEBUG] Processing {} unique trace pools", analytic_copper_pools.len());
    
    for ((z_min_nm, z_max_nm, material_id, net_id), paths) in analytic_copper_pools {
        let material_name = space
            .material_registry
            .get_name(material_id)
            .expect(&format!("Material ID {:?} not found in registry for copper pool", material_id));

        eprintln!(
            "[UNION POOL] key=(z_min={}, z_max={}, mat={:?}, net={:?}) → material='{}' ({} paths before union)",
            z_min_nm,
            z_max_nm,
            material_id,
            net_id,
            material_name,
            paths.len()
        );

        // Perform 2D Boolean Union to weld overlapping rectangles
        let unioned = clipper2_rust::union_64(&paths, &vec![], FillRule::NonZero);

        let z_min_mm = z_min_nm as f64 / 1_000_000.0;
        let depth_mm = (z_max_nm - z_min_nm) as f64 / 1_000_000.0;
        
        eprintln!(
            "[MESH TRACE DEBUG]     Extruding {} contours at Z={}mm depth={}mm",
            unioned.len(),
            z_min_mm,
            depth_mm
        );

        for (c_idx, path) in unioned.iter().enumerate() {
            if path.len() >= 3 {
                let outer_points: Vec<(f64, f64)> = path
                    .iter()
                    .map(|pt| (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0))
                    .collect();

                meshes.push(extrude_polygon_mesh(
                    &format!("Analytic_Route_Net_{}_Contour_{}", net_id.raw(), c_idx),
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

    // Render via barrel tubes (ONLY for drilled/plated PCB vias)
    // IC vias (deposited) are already handled natively via Contact substrate layers in the copper pool
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
        
        // Query the material registry for the manufacturing process
        let is_ic_via = space
            .material_registry
            .get_process(via.material_id)
            .map(|process| process == hwc_engine::ManufacturingProcess::Deposited)
            .unwrap_or(false);
        
        if is_ic_via {
            // SKIP: IC vias are fully represented by SubstrateLayerType::Contact in the entity graph,
            // which we already extruded natively via analytic_copper_pools.
            continue;
        }

        eprintln!("[EXPORT] Via {} uses plated material '{}' - rendering as PCB via with pads", via_idx, material_name);
        // PCB via: Plated through-hole with annular pads
        let pad_dia_mm = (via.diameter_nm + 2 * via.annular_ring_nm.max(via.diameter_nm / 4))
            as f64
            / 1_000_000.0;
        let barrel_thickness_mm = outer_dia_mm;

        meshes.push(create_via_mesh(ViaMeshParams {
            name: format!("PCB_Via_{}", via_idx),
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
