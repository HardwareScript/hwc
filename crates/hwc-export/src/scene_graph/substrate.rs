//! Substrate layer processing and net-aware clustering

use crate::geometry_union::{circle_to_path, rect_to_path};
use crate::mesh_extrusion::extrude_polygon_mesh;
use clipper2_rust::core::FillRule;
use super::mesh_generation::{
    create_box_with_holes_mesh, create_cylinder_mesh, create_via_mesh,
    CutoutParams,
};
use super::types::{BoxParams, FaceCulling, MaterialNode, MeshNode};
use hwc_engine::voxel_grid::CapType;
use compact_str::CompactString;
use hwc_engine::voxel_grid::SubstrateLayerShape;
use hwc_engine::HardwareSpace;
use hwc_parser::ProfileDefinition;
use rustc_hash::FxHashMap;

/// Add substrate mesh (FR4 base) from actual substrate layers
pub fn add_substrate(
    meshes: &mut Vec<MeshNode>,
    space: &HardwareSpace,
    materials: &FxHashMap<CompactString, MaterialNode>,
    _profile: Option<&ProfileDefinition>,
) {
    let mut substrate_layers = space.voxel_grid.get_substrate_layers().to_vec();

    // v0.1.7: Ensure a base substrate exists
    let has_base_substrate = substrate_layers.iter().any(|l| {
        let mat_name = space.material_registry.get_name(l.material).unwrap_or("");
        (l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour || 
         l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Substrate) 
         && mat_name != "Void"
    });

    if !has_base_substrate {
        let width_nm = space.dimensions.width_nm;
        let height_nm = space.dimensions.height_nm;
        let depth_nm = space.dimensions.depth_nm;

        let base_bbox = hwc_engine::geometry::BoundingBox::new(
            hwc_engine::geometry::Point3D::new(0, 0, 0),
            hwc_engine::geometry::Point3D::new(width_nm, height_nm, depth_nm),
        );

        let base_layer = hwc_engine::voxel_grid::SubstrateLayer::new(
            space.substrate_material_id,
            0,
            base_bbox,
            hwc_engine::voxel_grid::SubstrateLayerType::Substrate,
        );
        substrate_layers.push(base_layer);
    }

    let drills: Vec<_> = substrate_layers.iter()
        .filter(|l| l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Contact)
        .cloned()
        .collect();

    let mut layer_precedences = Vec::with_capacity(substrate_layers.len());
    for layer in &substrate_layers {
        let mat_name = space.material_registry.get_name(layer.material).unwrap_or("");
        let precedence = materials.get(mat_name).map(|m| m.precedence).unwrap_or(4);
        layer_precedences.push(precedence);
    }

    let original_layers = substrate_layers.clone();
    for (i, layer) in substrate_layers.iter_mut().enumerate() {
        let my_precedence = layer_precedences[i];

        for (j, other) in original_layers.iter().enumerate() {
            if i == j {
                continue;
            }
            let other_precedence = layer_precedences[j];
            let mut should_subtract = other_precedence < my_precedence || 
                (other_precedence == my_precedence && j > i && layer.material == other.material);

            // v0.1.9: PURE GEOMETRIC CONCENTRIC CHECK
            // If the XY center coordinates of two layers match exactly, they are 
            // concentric components of the same via structure. We bypass subtraction.
            let concentric = {
                let cx1 = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let cy1 = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let cx2 = (other.bbox.min.x + other.bbox.max.x) / 2;
                let cy2 = (other.bbox.min.y + other.bbox.max.y) / 2;
                (cx1 - cx2).abs() < 1000 && (cy1 - cy2).abs() < 1000
            };

            if concentric {
                should_subtract = false;
            }

            if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour 
                && layer.net != 0 
                && layer.net == other.net 
            {
                should_subtract = false;
            }

            if should_subtract {
                if layer.bbox.intersects(&other.bbox) {
                    match other.shape {
                        SubstrateLayerShape::Tube { outer_diameter, .. } => {
                            layer.add_cylinder_cutout(other.bbox, outer_diameter as i64);
                        }
                        SubstrateLayerShape::Cylinder { diameter, .. } => {
                            layer.add_cylinder_cutout(other.bbox, diameter);
                        }
                        _ => {
                            layer.add_cutout(other.bbox);
                        }
                    }
                }
            }
        }

        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Substrate
            || layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour
        {
            for drill in &drills {
                if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour 
                    && layer.net != 0 
                    && layer.net == drill.net 
                {
                    continue;
                }

                // Skip if the FR4 substrate slice is concentric with a drill
                let concentric = {
                    let cx1 = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                    let cy1 = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                    let cx2 = (drill.bbox.min.x + drill.bbox.max.x) / 2;
                    let cy2 = (drill.bbox.min.y + drill.bbox.max.y) / 2;
                    (cx1 - cx2).abs() < 1000 && (cy1 - cy2).abs() < 1000
                };

                if concentric {
                    continue;
                }

                match drill.shape {
                    SubstrateLayerShape::Tube { outer_diameter, .. } => {
                        layer.add_cylinder_cutout(drill.bbox, outer_diameter as i64);
                    }
                    SubstrateLayerShape::Cylinder { diameter, .. } => {
                        layer.add_cylinder_cutout(drill.bbox, diameter);
                    }
                    _ => {
                        layer.add_cutout(drill.bbox);
                    }
                }
            }
        }
    }

    fn nm_to_mm_precise(nm: i64) -> f64 {
        let mm_whole = nm / 1_000_000;
        let nm_remainder = nm % 1_000_000;
        mm_whole as f64 + (nm_remainder as f64 / 1_000_000.0)
    }

    // --- STRATEGY A: NET-AWARE COPPER UNIONING POOL ---
    let mut copper_pools: FxHashMap<(i64, i64, hwc_engine::voxel_grid::MaterialId, u32), Vec<clipper2_rust::Path64>> = FxHashMap::default();

    // 1. Gather all copper pours (Traces)
    for layer in &substrate_layers {
        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour && layer.net != 0 {
            let key = (layer.bbox.min.z, layer.bbox.max.z, layer.material, layer.net);
            let path = match layer.shape {
                SubstrateLayerShape::Rect => rect_to_path(&layer.bbox),
                _ => continue,
            };
            copper_pools.entry(key).or_default().push(path);
        }
    }

    // 2. Extract and inject via caps (Annular Rings) into the copper pools
    for layer in &substrate_layers {
        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Contact && layer.net != 0 {
            if let SubstrateLayerShape::Tube { outer_diameter: _, inner_diameter, pad_diameter, top_cap, bottom_cap, .. } = layer.shape {
                let cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let pad_radius = pad_diameter as i64 / 2;
                let inner_radius = inner_diameter as i64 / 2;

                let copper_thickness = 35_000;

                if top_cap != CapType::None {
                    let target_key = (layer.bbox.max.z - copper_thickness, layer.bbox.max.z, layer.material, layer.net);
                    let pool = copper_pools.entry(target_key).or_default();
                    
                    // Add outer pad boundary
                    pool.push(circle_to_path(cx, cy, pad_radius, 64));
                    
                    // v0.1.8 FIXED: For annular rings, add reversed inner hole path
                    if top_cap == CapType::Annular {
                        let mut hole_path = circle_to_path(cx, cy, inner_radius, 64);
                        hole_path.reverse();
                        pool.push(hole_path);
                    }
                }

                if bottom_cap != CapType::None {
                    let target_key = (layer.bbox.min.z, layer.bbox.min.z + copper_thickness, layer.material, layer.net);
                    let pool = copper_pools.entry(target_key).or_default();
                    
                    // Add outer pad boundary
                    pool.push(circle_to_path(cx, cy, pad_radius, 64));
                    
                    // v0.1.8 FIXED: For annular rings, add reversed inner hole path
                    if bottom_cap == CapType::Annular {
                        let mut hole_path = circle_to_path(cx, cy, inner_radius, 64);
                        hole_path.reverse();
                        pool.push(hole_path);
                    }
                }
            }
        }
    }

    // --- RENDER PASSES ---
    for (idx, layer) in substrate_layers.iter().enumerate() {
        let my_precedence = layer_precedences[idx];
        let material_name = space.material_registry.get_name(layer.material).unwrap_or("Unknown");

        if material_name == "Void" || material_name == "Air" {
            continue;
        }

        // Skip copper pours (traces) standalone, as they are processed in the unioned pool below
        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour && layer.net != 0 {
            continue;
        }

        let mut base_culling = FaceCulling::none();
        for (other_idx, other) in original_layers.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            let other_precedence = layer_precedences[other_idx];
            
            // v0.1.9: PURE GEOMETRIC CONCENTRIC CULLING EXEMPTION
            let is_concentric = {
                let cx1 = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let cy1 = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let cx2 = (other.bbox.min.x + other.bbox.max.x) / 2;
                let cy2 = (other.bbox.min.y + other.bbox.max.y) / 2;
                (cx1 - cx2).abs() < 1000 && (cy1 - cy2).abs() < 1000
            };

            if is_concentric {
                continue;
            }

            let mut should_cull_bottom = false;
            let mut should_cull_top = false;

            if my_precedence > other_precedence {
                should_cull_bottom = true;
                should_cull_top = true;
            } else if my_precedence == other_precedence 
                && layer.material == other.material 
                && layer.net != 0 
                && layer.net == other.net 
            {
                let bounding_boxes_match = 
                    (layer.bbox.min.x - other.bbox.min.x).abs() < 1000 
                    && (layer.bbox.max.x - other.bbox.max.x).abs() < 1000 
                    && (layer.bbox.min.y - other.bbox.min.y).abs() < 1000 
                    && (layer.bbox.max.y - other.bbox.max.y).abs() < 1000;

                if bounding_boxes_match {
                    should_cull_top = true;
                }
            }

            if should_cull_bottom || should_cull_top {
                let touching_bottom = (layer.bbox.min.z - other.bbox.max.z).abs() < 1000;
                let touching_top = (layer.bbox.max.z - other.bbox.min.z).abs() < 1000;

                if touching_bottom || touching_top {
                    if layer.bbox.min.x < other.bbox.max.x
                        && layer.bbox.max.x > other.bbox.min.x
                        && layer.bbox.min.y < other.bbox.max.y
                        && layer.bbox.max.y > other.bbox.min.y
                    {
                        if touching_bottom && should_cull_bottom {
                            base_culling.bottom = true;
                        }
                        if touching_top && should_cull_top {
                            base_culling.top = true;
                        }
                    }
                }
            }
        }

        let min_x_mm = nm_to_mm_precise(layer.bbox.min.x);
        let min_y_mm = nm_to_mm_precise(layer.bbox.min.y);
        let min_z_mm = nm_to_mm_precise(layer.bbox.min.z);

        let max_x_mm = nm_to_mm_precise(layer.bbox.max.x);
        let max_y_mm = nm_to_mm_precise(layer.bbox.max.y);
        let max_z_mm = nm_to_mm_precise(layer.bbox.max.z);

        let width = max_x_mm - min_x_mm;
        let height = max_y_mm - min_y_mm;
        let depth = max_z_mm - min_z_mm;

        match layer.shape {
            SubstrateLayerShape::Cylinder { diameter, segments } => {
                let diameter_mm = diameter as f64 / 1_000_000.0;
                let center_x_mm = (min_x_mm + max_x_mm) / 2.0;
                let center_y_mm = (min_y_mm + max_y_mm) / 2.0;

                meshes.push(create_cylinder_mesh(
                    &format!("Contact_{}", idx),
                    (center_x_mm, center_y_mm, min_z_mm),
                    diameter_mm,
                    depth,
                    segments,
                    material_name,
                    space.view,
                    base_culling,
                ));
            }
            SubstrateLayerShape::Tube {
                outer_diameter,
                inner_diameter,
                pad_diameter,
                segments,
                top_cap,
                bottom_cap,
                bottom_outer_diameter,
            } => {
                let outer_diameter_mm = outer_diameter as f64 / 1_000_000.0;
                let inner_diameter_mm = inner_diameter as f64 / 1_000_000.0;
                let pad_diameter_mm = pad_diameter as f64 / 1_000_000.0;
                let bottom_outer_diameter_mm = bottom_outer_diameter.map(|d| d as f64 / 1_000_000.0);
                let center_x_mm = (min_x_mm + max_x_mm) / 2.0;
                let center_y_mm = (min_y_mm + max_y_mm) / 2.0;

                meshes.push(create_via_mesh(
                    &format!("Bare_Via_Tube_{}", idx),
                    (center_x_mm, center_y_mm, min_z_mm),
                    outer_diameter_mm,
                    pad_diameter_mm,
                    (outer_diameter_mm - inner_diameter_mm) / 2.0,
                    depth,
                    segments,
                    top_cap,
                    bottom_cap,
                    bottom_outer_diameter_mm,
                    material_name,
                    space.view,
                ));
            }
            SubstrateLayerShape::Rect => {
                let mut z_boundaries = vec![layer.bbox.min.z, layer.bbox.max.z];
                for cutout in &layer.cutouts {
                    if cutout.bbox.min.z > layer.bbox.min.z && cutout.bbox.min.z < layer.bbox.max.z {
                        z_boundaries.push(cutout.bbox.min.z);
                    }
                    if cutout.bbox.max.z > layer.bbox.min.z && cutout.bbox.max.z < layer.bbox.max.z {
                        z_boundaries.push(cutout.bbox.max.z);
                    }
                }
                z_boundaries.sort();
                z_boundaries.dedup();

                for i in 0..(z_boundaries.len() - 1) {
                    let z_start = z_boundaries[i];
                    let z_end = z_boundaries[i + 1];
                    let slice_depth = nm_to_mm_precise(z_end - z_start);
                    let z_min_mm = nm_to_mm_precise(z_start);

                    let mut slice_cutouts = Vec::new();
                    for cutout in &layer.cutouts {
                        if cutout.bbox.min.z < z_end && cutout.bbox.max.z > z_start {
                            match cutout.shape {
                                SubstrateLayerShape::Cylinder { diameter, .. } => {
                                    let cx = (nm_to_mm_precise(cutout.bbox.min.x) + nm_to_mm_precise(cutout.bbox.max.x)) / 2.0;
                                    let cy = (nm_to_mm_precise(cutout.bbox.min.y) + nm_to_mm_precise(cutout.bbox.max.y)) / 2.0;
                                    let dia = diameter as f64 / 1_000_000.0;
                                    slice_cutouts.push(CutoutParams::Cylinder {
                                        cx,
                                        cy,
                                        dia,
                                        z_min: nm_to_mm_precise(cutout.bbox.min.z),
                                        z_max: nm_to_mm_precise(cutout.bbox.max.z),
                                    });
                                }
                                SubstrateLayerShape::Rect => {
                                    let x1 = nm_to_mm_precise(cutout.bbox.min.x);
                                    let y1 = nm_to_mm_precise(cutout.bbox.min.y);
                                    let x2 = nm_to_mm_precise(cutout.bbox.max.x);
                                    let y2 = nm_to_mm_precise(cutout.bbox.max.y);
                                    slice_cutouts.push(CutoutParams::Rect {
                                        x1,
                                        y1,
                                        x2,
                                        y2,
                                        z_min: nm_to_mm_precise(cutout.bbox.min.z),
                                        z_max: nm_to_mm_precise(cutout.bbox.max.z),
                                    });
                                }
                                _ => {}
                            }
                        }
                    }

                    let mut slice_culling = FaceCulling::none();
                    if z_start == layer.bbox.min.z {
                        slice_culling.bottom = base_culling.bottom;
                    }
                    if z_end == layer.bbox.max.z {
                        slice_culling.top = base_culling.top;
                    }

                    eprintln!("[DEBUG substrate-slice] Slice {}-{} depth: {} mm at z: {} mm. Cutouts: {}", idx, i, slice_depth, z_min_mm, slice_cutouts.len());
                    meshes.push(create_box_with_holes_mesh(
                        &format!("Substrate_Layer_{}_Z{}", idx, i),
                        BoxParams::new(min_x_mm, min_y_mm, z_min_mm, width, height, slice_depth),
                        slice_cutouts,
                        material_name,
                        space.view,
                        slice_culling,
                    ));
                }
            }
        }
    }

    // --- EXTRUDE UNIONED COPPER POOLS ---
    for ((z_min_nm, z_max_nm, material_id, net_raw), paths) in copper_pools {
        let material_name = space.material_registry.get_name(material_id).unwrap_or("Copper");
        
        let union_result = clipper2_rust::union_64(&paths, &Vec::new(), FillRule::NonZero);
        if !union_result.is_empty() {
            let mut outer_contours = Vec::new();
            let mut holes = Vec::new();

            for path in union_result {
                let mut points = Vec::new();
                for pt in &path {
                    points.push((pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0));
                }

                if clipper2_rust::is_positive(&path) {
                    outer_contours.push(points);
                } else {
                    holes.push(points);
                }
            }

            let z_min_mm = z_min_nm as f64 / 1_000_000.0;
            let depth_mm = (z_max_nm - z_min_nm) as f64 / 1_000_000.0;

            for (idx, outer) in outer_contours.iter().enumerate() {
                meshes.push(extrude_polygon_mesh(
                    &format!("Unified_Net_{}_Island_{}", net_raw, idx),
                    outer,
                    &holes,
                    z_min_mm,
                    depth_mm,
                    material_name,
                    space.view,
                ));
            }
        }
    }
}
