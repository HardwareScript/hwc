pub(crate) mod boundary_resolution;
pub(crate) mod endpoint_resolution;
pub(crate) mod net_registration;
pub(crate) mod path_utils;
pub(crate) mod pin_resolution;

pub use boundary_resolution::{resolve_route_boundary_points, resolve_route_pin_centers};
pub use endpoint_resolution::{
    construct_entity_name, endpoint_label, evaluate_index_expression, resolve_endpoint_entity_ids,
};
pub use net_registration::register_net_for_route;
pub use path_utils::{
    manhattan_path_to_segments, needs_automatic_routing, require_min_segment_length_nm,
};
pub use pin_resolution::get_pin_ids;

#[cfg(test)]
mod tests {
    use super::path_utils::manhattan_path_to_segments;
    use hwc_engine::Point3D;

    #[test]
    fn preserves_turns_above_pdk_threshold() {
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(100_000, 0, 0),
            Point3D::new(100_000, 50_000, 0),
            Point3D::new(200_000, 50_000, 0),
        ];
        let segs = manhattan_path_to_segments(&path, 180);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[1].start, Point3D::new(100_000, 0, 0));
        assert_eq!(segs[1].end, Point3D::new(100_000, 50_000, 0));
    }

    #[test]
    fn drops_turns_below_pdk_threshold() {
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(100_000, 0, 0),
            Point3D::new(100_000, 50_000, 0),
            Point3D::new(200_000, 50_000, 0),
        ];
        let segs = manhattan_path_to_segments(&path, 200_000);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn merges_collinear_runs() {
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(100_000, 0, 0),
            Point3D::new(200_000, 0, 0),
            Point3D::new(200_000, 100_000, 0),
        ];
        let segs = manhattan_path_to_segments(&path, 180);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start, Point3D::new(0, 0, 0));
        assert_eq!(segs[0].end, Point3D::new(200_000, 0, 0));
    }
}
