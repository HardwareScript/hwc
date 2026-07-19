use clipper2_rust::{EndType, JoinType, Path64, Paths64, Point64};
use hwc_engine::geometry::{BoundingBox, Point3D};

/// Convert an axis-aligned bounding box to a closed Clipper path (in nanometers)
pub fn rect_to_path(bbox: &BoundingBox) -> Path64 {
    vec![
        Point64::new(bbox.min.x, bbox.min.y),
        Point64::new(bbox.max.x, bbox.min.y),
        Point64::new(bbox.max.x, bbox.max.y),
        Point64::new(bbox.min.x, bbox.max.y),
    ]
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

/// Generate a 2D rectangular envelope around a continuous trace segment.
/// This preserves the exact geometry of the router output without voxel quantization.
///
/// For a segment from (x1, y1) to (x2, y2) with width w, this generates a closed
/// 4-vertex polygon representing the swept rectangle. Handles arbitrary angles
/// including 45° miters without aliasing.
pub fn trace_segment_to_path(start: Point3D, end: Point3D, width_nm: i64) -> Path64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;

    // Calculate perpendicular offset for trace width
    let len = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();

    if len < 1.0 {
        // Degenerate segment - return a small square
        let half_w = width_nm / 2;
        return vec![
            Point64::new(start.x - half_w, start.y - half_w),
            Point64::new(start.x + half_w, start.y - half_w),
            Point64::new(start.x + half_w, start.y + half_w),
            Point64::new(start.x - half_w, start.y + half_w),
        ];
    }

    let half_w = width_nm as f64 / 2.0;

    // Perpendicular unit vector (rotated 90°)
    let nx = -dy as f64 / len * half_w;
    let ny = dx as f64 / len * half_w;

    // Four corners of the swept rectangle
    vec![
        Point64::new((start.x as f64 + nx) as i64, (start.y as f64 + ny) as i64),
        Point64::new((end.x as f64 + nx) as i64, (end.y as f64 + ny) as i64),
        Point64::new((end.x as f64 - nx) as i64, (end.y as f64 - ny) as i64),
        Point64::new((start.x as f64 - nx) as i64, (start.y as f64 - ny) as i64),
    ]
}

/// Generate a perfectly mitered trace outline from a sequence of route segments using
/// Clipper2's native path offsetting engine (the "Font-Engine" paradigm).
///
/// This approach treats the routed path as a continuous 1D polyline and uses mathematical
/// angle bisectors to generate perfect miter joins, eliminating segment-by-segment welding
/// artifacts, notched corners, and coordinate rounding errors.
///
/// Returns a Paths64 (vector of closed polygons) representing the stroked trace outline.
pub fn stroke_route_segments(
    segments: &[hwc_engine::space::LineSegment],
    width_nm: i64,
) -> Paths64 {
    if segments.is_empty() {
        return Paths64::new();
    }

    // Build a single, continuous 1D polyline of waypoints
    let mut path1d = Path64::new();

    // Add the start point of the first segment
    if let Some(first_seg) = segments.first() {
        path1d.push(Point64::new(first_seg.start.x, first_seg.start.y));
    }

    // Add the end point of each segment to form a continuous path
    for segment in segments {
        path1d.push(Point64::new(segment.end.x, segment.end.y));
    }

    if path1d.len() < 2 {
        return Paths64::new();
    }

    let paths_to_offset = vec![path1d];

    // STROKE THE PATH: Use Clipper2's native offsetting to generate perfect mitered outlines
    // - JoinType::Miter calculates exact angle bisectors at corners
    // - EndType::Square provides square end caps at start/goal
    // - Miter limit of 2.0 prevents infinite spikes at very sharp angles
    clipper2_rust::inflate_paths_64(
        &paths_to_offset,
        width_nm as f64 / 2.0, // Delta offset (half-width)
        JoinType::Miter,       // Perfect mitered corners
        EndType::Square,       // Square end caps
        2.0,                   // Miter limit
        0.0,                   // Precision (0.0 = auto)
    )
}
