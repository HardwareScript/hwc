//! Substrate layer processing and net-aware clustering

use super::geometry::douglas_peucker;
use super::mesh_generation::{
    create_box_with_holes_mesh, create_cylinder_mesh, create_tube_mesh, create_via_mesh,
    CutoutParams,
};
use super::ribbon::create_extruded_ribbon;
use super::types::{BoxParams, FaceCulling, MaterialNode, MeshNode};
use crate::contour_tracer::{ContourConfig, ContourTracer};
use compact_str::CompactString;
use hwc_engine::voxel_grid::SubstrateLayerShape;
use hwc_engine::HardwareSpace;
use hwc_parser::ProfileDefinition;
use rustc_hash::FxHashMap;

/// Add substrate mesh (FR4 base) from actual substrate layers
///
/// If export constraints enable anti-aliasing or if layers have complex geometry,
/// uses ContourTracer for smooth voxel-to-vector conversion.
///
/// **NET-AWARE CLUSTERING**: Groups layers by (net, material, z-layer) before export.
/// This ensures that multiple pours on the same net are merged into a single smooth shape.
pub fn add_substrate(
    meshes: &mut Vec<MeshNode>,
    space: &HardwareSpace,
    materials: &FxHashMap<CompactString, MaterialNode>,
    profile: Option<&ProfileDefinition>,
) {
    // Export substrate layers from the voxel grid's sparse representation
    let mut substrate_layers = space.voxel_grid.get_substrate_layers().to_vec();

    // v0.1.7: Ensure a base substrate exists if dimensions are defined but no Substrate layer is present
    let has_base_substrate = substrate_layers.iter().any(|l| {
        let mat_name = space.material_registry.get_name(l.material).unwrap_or("");
        (l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour || 
         l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Substrate) 
         && mat_name != "Void"
    });

    // 1. Ensure base substrate exists
    if !has_base_substrate {
        let width_nm = space.dimensions.width_nm;
        let height_nm = space.dimensions.height_nm;
        let depth_nm = space.dimensions.depth_nm;

        let base_bbox = hwc_engine::geometry::BoundingBox::new(
            hwc_engine::geometry::Point3D::new(0, 0, 0),
            hwc_engine::geometry::Point3D::new(width_nm, height_nm, depth_nm),
        );

        // Add a virtual substrate layer for the board
        let base_layer = hwc_engine::voxel_grid::SubstrateLayer::new(
            space.substrate_material_id,
            0,
            base_bbox,
            hwc_engine::voxel_grid::SubstrateLayerType::Substrate,
        );
        substrate_layers.push(base_layer);
    }

    // 2. Extract all contacts/drills to use as cutters (v0.1.7)
    let drills: Vec<_> = substrate_layers.iter()
        .filter(|l| l.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Contact)
        .cloned()
        .collect();

    // 3. v0.1.7: MANIFOLD PUNCH-THROUGH (The "Lego-Plug" Innovation)
    // Instead of ghost geometry, we calculate shared boundaries and subtract
    // higher-precedence materials from lower ones.
    let mut layer_precedences = Vec::with_capacity(substrate_layers.len());

    for layer in &substrate_layers {
        let mat_name = space.material_registry.get_name(layer.material).unwrap_or("");
        let precedence = materials.get(mat_name).map(|m| m.precedence).unwrap_or(4);
        layer_precedences.push(precedence);
    }

    let original_layers = substrate_layers.clone();
    for (i, layer) in substrate_layers.iter_mut().enumerate() {
        let my_precedence = layer_precedences[i];

        // 3a. Subtract higher-precedence layers from this one
        for (j, other) in original_layers.iter().enumerate() {
            if i == j {
                continue;
            }
            let other_precedence = layer_precedences[j];

            // v0.1.7: MANIFOLD PUNCH-THROUGH (The "Lego-Plug" Innovation)
            // 1. If other material has HIGHER precedence (lower value), it punches a hole in me.
            // 2. If materials have SAME precedence, the one with HIGHER index (added later)
            //    punches a hole in the one with LOWER index. This ensures that traces
            //    (added after pours) correctly punch through pours to avoid Z-fighting.
            let should_subtract = other_precedence < my_precedence || 
                (other_precedence == my_precedence && j > i && layer.material == other.material);

            if should_subtract {
                // v0.1.7: MANIFOLD RULE
                // We consider it a cutout if it intersects OR touches our boundaries
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

        // 3b. Subtract drills (Highest Precedence / Void)
        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Substrate
            || layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour
        {
            for drill in &drills {
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

    // Determine if we should use contour tracing
    let export_constraints = profile.and_then(|p| p.export.as_ref());
    let use_contour_tracing = export_constraints
        .map(|ec| ec.antialiasing)
        .unwrap_or(false);

    // FIX A: INTEGER-NANOMETER EXPORT (Precise coordinate conversion)
    fn nm_to_mm_precise(nm: i64) -> f64 {
        let mm_whole = nm / 1_000_000;
        let nm_remainder = nm % 1_000_000;
        mm_whole as f64 + (nm_remainder as f64 / 1_000_000.0)
    }

    // Separate layers into analytic (v0.1.7) and clustered (legacy pours)
    let mut to_cluster = Vec::new();

    for (idx, layer) in substrate_layers.iter().enumerate() {
        let my_precedence = layer_precedences[idx];

        // v0.1.7: Conductive Unioning (The "Redundancy" Innovation)
        // If this layer is entirely contained within another layer of the same material
        // and same precedence, we skip it to prevent "Phantom Slicing" artifacts in GLB.
        let mut redundant = false;
        for (other_idx, other) in original_layers.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            if layer.material == other.material && my_precedence == layer_precedences[other_idx] {
                if other.bbox.contains_bbox(&layer.bbox) {
                    redundant = true;
                    break;
                }
            }
        }
        if redundant {
            eprintln!("[DEBUG unioning] Skipping redundant layer {} (contained within {})", idx, "other");
            continue;
        }

        // v0.1.7: Automatic Surface Culling (The "Handshake" Innovation)
        let mut base_culling = FaceCulling::none();
        for (other_idx, other) in original_layers.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            let other_precedence = layer_precedences[other_idx];

            // If I have HIGHER precedence (lower value), I cull my own face
            // if I'm touching a LOWER precedence material.
            if my_precedence < other_precedence {
                // Check for Z-touching (1um tolerance)
                let touching_bottom = (layer.bbox.min.z - other.bbox.max.z).abs() < 1000;
                let touching_top = (layer.bbox.max.z - other.bbox.min.z).abs() < 1000;

                if touching_bottom || touching_top {
                    // Check for XY overlap
                    if layer.bbox.min.x < other.bbox.max.x
                        && layer.bbox.max.x > other.bbox.min.x
                        && layer.bbox.min.y < other.bbox.max.y
                        && layer.bbox.max.y > other.bbox.min.y
                    {
                        if touching_bottom {
                            base_culling.bottom = true;
                        }
                        if touching_top {
                            base_culling.top = true;
                        }
                    }
                }
            }
        }

        let material_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");

        // Rule 3: Skip rendering "Void" or "Air" materials standalone
        if material_name == "Void" || material_name == "Air" {
            continue;
        }

        // Rule 2: Traces and Pours use clustering logic for smooth hulls
        if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour && use_contour_tracing {
            to_cluster.push(layer);
            continue;
        }

        // Rule 1: Analytic Path (The "One Truth")
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

                // Check if this cylinder has a central cutout (Annular Ring)
                if let Some(cutout) = layer.cutouts.first() {
                    match cutout.shape {
                        SubstrateLayerShape::Cylinder { diameter: inner_dia, .. } => {
                            let inner_diameter_mm = inner_dia as f64 / 1_000_000.0;
                            meshes.push(create_via_mesh(
                                &format!("Pad_{}", idx),
                                (center_x_mm, center_y_mm, min_z_mm),
                                inner_diameter_mm,
                                diameter_mm,
                                0.025, // Plating thickness 25um
                                depth,
                                segments,
                                material_name,
                                space.view,
                            ));
                            continue;
                        }
                        _ => {}
                    }
                }

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
                caps,
            } => {
                let outer_diameter_mm = outer_diameter as f64 / 1_000_000.0;
                let inner_diameter_mm = inner_diameter as f64 / 1_000_000.0;
                let pad_diameter_mm = pad_diameter as f64 / 1_000_000.0;
                let center_x_mm = (min_x_mm + max_x_mm) / 2.0;
                let center_y_mm = (min_y_mm + max_y_mm) / 2.0;

                if caps {
                    meshes.push(create_via_mesh(
                        &format!("Unified_Via_{}", idx),
                        (center_x_mm, center_y_mm, min_z_mm),
                        outer_diameter_mm,
                        pad_diameter_mm,
                        (outer_diameter_mm - inner_diameter_mm) / 2.0,
                        depth,
                        segments,
                        material_name,
                        space.view,
                    ));
                } else {
                    meshes.push(create_tube_mesh(
                        &format!("Bare_Tube_{}", idx),
                        (center_x_mm, center_y_mm, min_z_mm),
                        outer_diameter_mm,
                        inner_diameter_mm,
                        depth,
                        segments,
                        false,
                        material_name,
                        space.view,
                    ));
                }
            }
            SubstrateLayerShape::Rect => {
                // Z-Slicing logic to handle profiled holes
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
                        // v0.1.7: MANIFOLD CULLING RULE
                        // A cutout affects this slice if:
                        // 1. It overlaps the Z-range of the slice (Volume Punch)
                        // 2. It touches the Z-boundaries of the slice (Surface Punch)
                        if cutout.bbox.min.z < z_end && cutout.bbox.max.z > z_start {
                            // Volume Punch (Hole)
                            match cutout.shape {
                                SubstrateLayerShape::Cylinder { diameter, .. } => {
                                    let cx = (nm_to_mm_precise(cutout.bbox.min.x)
                                        + nm_to_mm_precise(cutout.bbox.max.x))
                                        / 2.0;
                                    let cy = (nm_to_mm_precise(cutout.bbox.min.y)
                                        + nm_to_mm_precise(cutout.bbox.max.y))
                                        / 2.0;
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
                        } else if cutout.bbox.min.z == z_end || cutout.bbox.max.z == z_start {
                            // Surface Punch (Flicker Prevention)
                            // We pass these as cutouts so the mesher can cull the surface faces
                            match cutout.shape {
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

                    // Only apply base_culling to the outermost slices
                    let mut slice_culling = FaceCulling::none();
                    if z_start == layer.bbox.min.z {
                        slice_culling.bottom = base_culling.bottom;
                    }
                    if z_end == layer.bbox.max.z {
                        slice_culling.top = base_culling.top;
                    }

                    // v0.1.7 FIXED: Always use the hole-aware mesh generator for substrate layers
                    // This ensures that drills actually carve visible tunnels in the GLB.
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

    // Step 2: Handle clustering for smooth hulls
    if !to_cluster.is_empty() {
        let config = if let Some(ec) = export_constraints {
            let tolerance_nm = ec
                .smoothing_tolerance
                .as_ref()
                .map(|m| match &m.unit {
                    hwc_parser::Unit::Millimeter => m.value * 1_000_000.0,
                    hwc_parser::Unit::Micrometer => m.value * 1_000.0,
                    hwc_parser::Unit::Centimeter => m.value * 10_000_000.0,
                    _ => m.value,
                })
                .unwrap_or(100.0);

            let tolerance_voxels = (tolerance_nm / space.voxel_size.x_nm as f64).max(1.5);

            ContourConfig {
                antialiasing: ec.antialiasing,
                smoothing_tolerance: tolerance_voxels,
                corner_lock: ec.corner_lock.clone().unwrap_or_else(|| vec![45, 90]),
                smoothing_iterations: 4,
                simplification_tolerance: 0.5,
            }
        } else {
            ContourConfig::default()
        };

        let tracer = ContourTracer::new(config);
        add_substrate_with_net_clustering(meshes, space, &to_cluster, &tracer);
    }
}

/// Net-aware substrate export with clustering
fn add_substrate_with_net_clustering(
    meshes: &mut Vec<MeshNode>,
    space: &HardwareSpace,
    substrate_layers: &[&hwc_engine::voxel_grid::SubstrateLayer],
    _tracer: &ContourTracer,
) {
    // Step 1: Group layers by (net, material, z-layer)
    let mut clusters: FxHashMap<(u32, u8, i64, i64), Vec<&hwc_engine::voxel_grid::SubstrateLayer>> =
        FxHashMap::default();

    for layer in substrate_layers {
        let key = (
            layer.net,
            layer.material,
            layer.bbox.min.z,
            layer.bbox.max.z,
        );
        clusters.entry(key).or_default().push(layer);
    }

    for ((net_id, material_id, z_min, z_max), layers) in clusters.iter() {
        let material_name = space
            .material_registry
            .get_name(*material_id)
            .unwrap_or("Unknown");

        // Skip rendering "Void" materials
        if material_name == "Void" {
            continue;
        }

        let centers: Vec<(f64, f64)> = layers
            .iter()
            .map(|layer| {
                (
                    (layer.bbox.min.x + layer.bbox.max.x) as f64 / 2_000_000.0,
                    (layer.bbox.min.y + layer.bbox.max.y) as f64 / 2_000_000.0,
                )
            })
            .collect();

        if centers.is_empty() {
            continue;
        }

        let voxel_size_mm = if !layers.is_empty() {
            (layers[0].bbox.max.x - layers[0].bbox.min.x) as f64 / 1_000_000.0
        } else {
            0.5
        };

        let mut simplified = douglas_peucker(&centers, voxel_size_mm * 0.1);

        if simplified.len() >= 2 && !layers.is_empty() {
            // Snap logic omitted for brevity in this update call, assuming it remains the same
            // (Re-adding the snap logic properly)
            let c1 = simplified[0];
            let c2 = simplified[1];
            let dx = c1.0 - c2.0;
            let dy = c1.1 - c2.1;
            let mag = (dx * dx + dy * dy).sqrt();
            if mag > 0.0 {
                let first_bbox = &layers[0].bbox;
                let min_x = first_bbox.min.x as f64 / 1_000_000.0;
                let min_y = first_bbox.min.y as f64 / 1_000_000.0;
                let max_x = first_bbox.max.x as f64 / 1_000_000.0;
                let max_y = first_bbox.max.y as f64 / 1_000_000.0;
                let mut t = f64::MAX;
                if dx > 0.0 { t = t.min((max_x - c1.0) / dx); }
                if dx < 0.0 { t = t.min((min_x - c1.0) / dx); }
                if dy > 0.0 { t = t.min((max_y - c1.1) / dy); }
                if dy < 0.0 { t = t.min((min_y - c1.1) / dy); }
                simplified[0] = (c1.0 + dx * t, c1.1 + dy * t);
            }
            let n = simplified.len() - 1;
            let cn = simplified[n];
            let cn_1 = simplified[n - 1];
            let dx = cn.0 - cn_1.0;
            let dy = cn.1 - cn_1.1;
            let mag = (dx * dx + dy * dy).sqrt();
            if mag > 0.0 {
                let last_bbox = &layers[layers.len() - 1].bbox;
                let min_x = last_bbox.min.x as f64 / 1_000_000.0;
                let min_y = last_bbox.min.y as f64 / 1_000_000.0;
                let max_x = last_bbox.max.x as f64 / 1_000_000.0;
                let max_y = last_bbox.max.y as f64 / 1_000_000.0;
                let mut t = f64::MAX;
                if dx > 0.0 { t = t.min((max_x - cn.0) / dx); }
                if dx < 0.0 { t = t.min((min_x - cn.0) / dx); }
                if dy > 0.0 { t = t.min((max_y - cn.1) / dy); }
                if dy < 0.0 { t = t.min((min_y - cn.1) / dy); }
                simplified[n] = (cn.0 + dx * t, cn.1 + dy * t);
            }
        }

        let thickness = (*z_max - *z_min) as f64 / 1_000_000.0;
        let z_base = *z_min as f64 / 1_000_000.0;

        if simplified.len() >= 2 {
            if let Some(mesh) = create_extruded_ribbon(
                &format!("Net_{}_{}", net_id, material_name),
                &simplified,
                voxel_size_mm,
                thickness,
                z_base,
                material_name,
                space.view,
            ) {
                meshes.push(mesh);
            }
        }
    }
}
