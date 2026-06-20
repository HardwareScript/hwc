use crate::geometry::{BoundingBox, Point3D, TraceSegment};
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::topological_router::TopologicalRouter;

/// Route a net using the Topological Line-Search Router.
///
/// This is the sole routing engine. It projects orthogonal search rays from
/// start and target ports, using the Axis-Aligned Slab Method for O(log N)
/// ray-AABB intersection queries over the flat-packed spatial index.
///
/// Returns the path waypoints in continuous nanometer coordinates, or None
/// if no obstacle-free path exists.
pub fn route_net_deterministic(
    start: Point3D,
    goal: Point3D,
    params: &super::types::RoutingParams,
) -> Option<Vec<Point3D>> {
    let board_bounds = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(params.bounds.width_nm, params.bounds.height_nm, params.bounds.depth_nm),
    );

    let mut spatial_index = DynamicSpatialIndex::new();
    let mut seg_id = 0usize;

    if let Some(entity_graph) = params.entity_graph {
        for meta in entity_graph.get_component_metadata() {
            let w = meta.bbox.max.x - meta.bbox.min.x;
            let h = meta.bbox.max.y - meta.bbox.min.y;
            let trace_seg = TraceSegment::new(meta.bbox.min, meta.bbox.max, w.max(h));
            spatial_index.insert(IndexedSegment::new(seg_id, 0, &trace_seg, meta.bbox.min.z));
            seg_id += 1;
        }
    }

    for (&point, &net_id) in params.occupied_voxels {
        if net_id == params.net_id {
            continue;
        }
        let half = params.voxel_size.x_nm / 2;
        let trace_seg = TraceSegment::new(
            Point3D::new(point.x - half, point.y - half, point.z),
            Point3D::new(point.x + half, point.y + half, point.z),
            params.voxel_size.x_nm,
        );
        spatial_index.insert(IndexedSegment::new(
            seg_id,
            net_id.0 as usize,
            &trace_seg,
            point.z,
        ));
        seg_id += 1;
    }

    let trace_width = params.voxel_size.x_nm;
    let track_pitch = params.voxel_size.x_nm;
    let topo_router = TopologicalRouter::new(trace_width, track_pitch);

    topo_router
        .route(start, goal, &spatial_index, &board_bounds)
        .map(|topo_path| topo_path.waypoints)
}
