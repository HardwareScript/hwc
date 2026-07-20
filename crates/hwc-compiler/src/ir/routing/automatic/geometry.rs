//! Geometry routing: spatial index, path refinement, and segment creation.
//!
//! Phase 2 of the routing pipeline: builds the spatial index for obstacle
//! detection, runs the topological router, and creates trace segments.

use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::{HardwareSpace, LineSegment, Point3D};

/// Configuration for spatial index building.
pub struct SpatialIndexConfig<'a> {
    pub space: &'a HardwareSpace,
    pub from_component_name: CompactString,
    pub to_component_name: CompactString,
}

/// Build the spatial index for obstacle queries during routing.
///
/// Populates the index with substrate layers and component metadata,
/// excluding the start and goal components to allow routing from/to them.
pub fn build_spatial_index(
    config: &SpatialIndexConfig,
) -> hwc_engine::geometry_router::DynamicSpatialIndex {
    let mut idx = hwc_engine::geometry_router::DynamicSpatialIndex::new();

    if let Some(z_ranges) = config.space.entity_graph.spatial().layer_z_ranges() {
        idx.set_layer_z_ranges(&z_ranges);
    }

    for (layer_idx, layer) in config
        .space
        .entity_graph
        .get_substrate_layers()
        .iter()
        .enumerate()
    {
        let width = layer.bbox.max.x - layer.bbox.min.x;
        let height = layer.bbox.max.y - layer.bbox.min.y;
        let trace_seg = hwc_engine::geometry_router::IndexedSegment {
            source:
                hwc_engine::geometry_router::spatial_index::SpatialEntitySource::SubstrateLayer {
                    index: layer_idx,
                },
            segment_id: layer_idx,
            net_id: layer.net as usize,
            width_nm: width.max(height),
            thickness_nm: layer.bbox.max.z - layer.bbox.min.z,
            start: layer.bbox.min,
            end: layer.bbox.max,
            layer: layer.bbox.min.z,
        };
        eprintln!(
            "[AUTO ROUTE INDEX] Adding substrate layer {}: net_id={}, bbox=({},{},{}) to ({},{},{})",
            layer_idx, layer.net,
            layer.bbox.min.x, layer.bbox.min.y, layer.bbox.min.z,
            layer.bbox.max.x, layer.bbox.max.y, layer.bbox.max.z
        );
        idx.insert(trace_seg);
    }

    for meta in config.space.entity_graph.get_component_metadata() {
        if meta.name == config.from_component_name || meta.name == config.to_component_name {
            eprintln!(
                "[AUTO ROUTE INDEX] Skipping component '{}' (is start or goal)",
                meta.name
            );
            continue;
        }

        let width = meta.bbox.max.x - meta.bbox.min.x;
        let height = meta.bbox.max.y - meta.bbox.min.y;
        let trace_seg = hwc_engine::geometry_router::IndexedSegment {
            source:
                hwc_engine::geometry_router::spatial_index::SpatialEntitySource::ComponentInstance {
                    instance_id: 0,
                },
            segment_id: 0,
            net_id: 0,
            width_nm: width.max(height),
            thickness_nm: meta.bbox.max.z - meta.bbox.min.z,
            start: meta.bbox.min,
            end: meta.bbox.max,
            layer: meta.bbox.min.z,
        };
        idx.insert(trace_seg);
    }

    idx
}

/// Refine path Z-coordinates using the stackup manager.
///
/// Transforms the router's grid-snapped path back into exact physical layer heights,
/// eliminating discretization noise.
pub fn refine_path_z(
    mut path: Vec<Point3D>,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    target_z_nm: Option<i64>,
    start_pos_z: i64,
    resolution_nm: i64,
) -> Result<(Vec<Point3D>, i64), IrError> {
    let mut trace_thickness_nm = resolution_nm;

    if path.len() >= 2 {
        if let Some(fixed_z) = target_z_nm.or(Some(start_pos_z)) {
            if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(fixed_z) {
                trace_thickness_nm = stackup_manager.get_thickness_for_layer_index(layer_idx)?;
            }
        } else {
            for point in path.iter_mut() {
                if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(point.z) {
                    let true_z = stackup_manager.get_z_start_nm_for_layer_index(layer_idx)?;
                    trace_thickness_nm =
                        stackup_manager.get_thickness_for_layer_index(layer_idx)?;
                    point.z = true_z;
                }
            }
        }
    }

    Ok((path, trace_thickness_nm))
}

/// Create line segments from the refined path.
///
/// Handles via transitions for layer overrides and applies collinear merge
/// with PDK min_segment_length filter.
pub fn create_segments(
    refined_path: &[Point3D],
    start_boundary: Point3D,
    goal_boundary: Point3D,
    target_z_nm: Option<i64>,
    _trace_width_nm: i64,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Vec<LineSegment>, IrError> {
    let mut segs = Vec::new();

    if let Some(target_z) = target_z_nm {
        let pin_z = start_boundary.z;
        eprintln!(
            "[VIA TRANSITION DEBUG] Start: pin_z={}, target_z={}, diff={}",
            pin_z,
            target_z,
            (pin_z - target_z).abs()
        );

        if (pin_z - target_z).abs() > 50 {
            eprintln!(
                "  ✅ Adding START via transition: {} -> {}",
                pin_z, target_z
            );
            let start_up = Point3D::new(start_boundary.x, start_boundary.y, target_z);
            segs.push(hwc_engine::LineSegment::new(start_boundary, start_up));
        } else {
            eprintln!("  ⏭️  Skipping START via: pin already on target layer");
        }
    }

    if refined_path.len() >= 2 {
        let min_seg_len_nm = crate::ir::routing::helpers::require_min_segment_length_nm(profile)?;
        segs.extend(crate::ir::routing::helpers::manhattan_path_to_segments(
            refined_path,
            min_seg_len_nm,
        ));
    }

    if let Some(target_z) = target_z_nm {
        let pin_z = goal_boundary.z;
        eprintln!(
            "[VIA TRANSITION DEBUG] Goal: pin_z={}, target_z={}, diff={}",
            pin_z,
            target_z,
            (pin_z - target_z).abs()
        );

        if (pin_z - target_z).abs() > 50 {
            eprintln!("  ✅ Adding GOAL via transition: {} -> {}", target_z, pin_z);
            let goal_down = Point3D::new(goal_boundary.x, goal_boundary.y, target_z);
            segs.push(hwc_engine::LineSegment::new(goal_down, goal_boundary));
        } else {
            eprintln!("  ⏭️  Skipping GOAL via: pin already on target layer");
        }
    }

    Ok(segs)
}

/// Check non-routable layers in the path.
///
/// The topological router skips non-routable layers, but this post-route check
/// catches edge cases where the router may have slipped through.
pub fn check_non_routable_layers(
    path: &[Point3D],
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    if let Some(stackup) = profile.and_then(|p| p.stackup.as_ref()) {
        for point in path {
            if let Some(layer_name) = stackup_manager.get_layer_name_at_z(point.z) {
                if let Some(layer_def) = stackup.layers.iter().find(|l| l.name.name == layer_name) {
                    if let Some(hwc_parser::RoutableMode::False) = layer_def.routable {
                        let material = layer_def.material.clone();
                        return Err(IrError::NonRoutableLayer {
                            layer: layer_name.into(),
                            material,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Align path points to a common axis if start and goal share an axis.
///
/// Eliminates quantization noise and "bumps" for straight routes.
pub fn align_path_to_axis(path: &mut [Point3D], start_boundary: Point3D, goal_boundary: Point3D) {
    if start_boundary.x == goal_boundary.x {
        for point in path.iter_mut() {
            point.x = start_boundary.x;
        }
    } else if start_boundary.y == goal_boundary.y {
        for point in path.iter_mut() {
            point.y = start_boundary.y;
        }
    }
}
