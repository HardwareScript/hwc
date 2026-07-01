pub use hwc_physics::spatial_index::*;

use hwc_physics::geometry::{BoundingBox, Point3D};

/// Query the index for all segments that overlap a given segment's bounding box.
/// Excludes segments with the same net_id (same-net segments are handled separately).
pub fn query_overlapping_segments<'a>(
    index: &'a DynamicSpatialIndex,
    segment: &IndexedSegment,
    clearance_nm: i64,
) -> Vec<&'a IndexedSegment> {
    let expanded_bbox = BoundingBox {
        min: Point3D::new(
            segment.start.x.min(segment.end.x) - segment.width_nm / 2 - clearance_nm,
            segment.start.y.min(segment.end.y) - segment.width_nm / 2 - clearance_nm,
            segment.start.z.min(segment.end.z),
        ),
        max: Point3D::new(
            segment.start.x.max(segment.end.x) + segment.width_nm / 2 + clearance_nm,
            segment.start.y.max(segment.end.y) + segment.width_nm / 2 + clearance_nm,
            segment.start.z.max(segment.end.z),
        ),
    };

    index
        .query_bbox(&expanded_bbox)
        .into_iter()
        .filter(|s| s.net_id != segment.net_id)
        .collect()
}
