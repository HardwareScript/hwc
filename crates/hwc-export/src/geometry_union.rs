use hwc_engine::geometry::BoundingBox;
use clipper2_rust::{Path64, Point64};

/// Convert an axis-aligned bounding box to a closed Clipper path (in nanometers)
pub fn rect_to_path(bbox: &BoundingBox) -> Path64 {
    let mut path = Path64::new();
    path.push(Point64::new(bbox.min.x, bbox.min.y));
    path.push(Point64::new(bbox.max.x, bbox.min.y));
    path.push(Point64::new(bbox.max.x, bbox.max.y));
    path.push(Point64::new(bbox.min.x, bbox.max.y));
    path
}

/// Convert a circular via landing pad into a 64-sided regular polygon (in nanometers)
pub fn circle_to_path(cx: i64, cy: i64, radius: i64, segments: usize) -> Path64 {
    let mut path = Path64::new();
    for i in 0..segments {
        let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
        let x = cx + (radius as f64 * angle.cos()) as i64;
        let y = cy + (radius as f64 * angle.sin()) as i64;
        path.push(Point64::new(x, y));
    }
    path
}
