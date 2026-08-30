//! 2D Boundary Copper Welding Subsystem
//!
//! Executes Clipper2 Non-Zero Winding 2D Boolean union strictly at the export boundary,
//! keeping internal database representations fast and granular while guaranteeing
//! monolithic polygon geometry for manufacturing mask generation.

use clipper2_rust::{EndType, FillRule, JoinType, Path64, Paths64, Point64};
use hwc_engine::geometry::{BoundingBox, Point3D};

/// Convert an axis-aligned bounding box to a closed Clipper path in picometers/nanometers.
pub fn rect_to_path(bbox: &BoundingBox) -> Path64 {
    vec![
        Point64::new(bbox.min.x, bbox.min.y),
        Point64::new(bbox.max.x, bbox.min.y),
        Point64::new(bbox.max.x, bbox.max.y),
        Point64::new(bbox.min.x, bbox.max.y),
    ]
}

/// Convert a circular via or pin landing pad into a regular polygon.
pub fn circle_to_path(cx: i64, cy: i64, radius: i64, segments: usize) -> Path64 {
    let mut path = Path64::new();
    let num_segs = segments.max(8);
    for i in 0..num_segs {
        let angle = (i as f64 / num_segs as f64) * 2.0 * std::f64::consts::PI;
        let x = cx + (radius as f64 * angle.cos()) as i64;
        let y = cy + (radius as f64 * angle.sin()) as i64;
        path.push(Point64::new(x, y));
    }
    path
}

/// Generate a 2D rectangular envelope around a continuous trace segment.
pub fn trace_segment_to_path(start: Point3D, end: Point3D, width: i64) -> Path64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();

    if len < 1.0 {
        let half_w = width / 2;
        return vec![
            Point64::new(start.x - half_w, start.y - half_w),
            Point64::new(start.x + half_w, start.y - half_w),
            Point64::new(start.x + half_w, start.y + half_w),
            Point64::new(start.x - half_w, start.y + half_w),
        ];
    }

    let half_w = width as f64 / 2.0;
    let nx = -dy as f64 / len * half_w;
    let ny = dx as f64 / len * half_w;

    vec![
        Point64::new((start.x as f64 + nx) as i64, (start.y as f64 + ny) as i64),
        Point64::new((end.x as f64 + nx) as i64, (end.y as f64 + ny) as i64),
        Point64::new((end.x as f64 - nx) as i64, (end.y as f64 - ny) as i64),
        Point64::new((start.x as f64 - nx) as i64, (start.y as f64 - ny) as i64),
    ]
}

/// Executes Clipper2 Non-Zero Winding 2D Boolean union across a collection of input polygons.
pub fn weld_copper_geometry(input_paths: &[Path64]) -> Paths64 {
    if input_paths.is_empty() {
        return Paths64::new();
    }

    let subject: Paths64 = input_paths.to_vec();
    let empty_clip = Paths64::new();

    clipper2_rust::union_64(&subject, &empty_clip, FillRule::NonZero)
}

/// Generates a mitered continuous trace outline using Clipper2 path inflation.
pub fn stroke_polyline(points: &[Point3D], width: i64) -> Paths64 {
    if points.len() < 2 {
        return Paths64::new();
    }

    let mut path = Path64::new();
    for pt in points {
        path.push(Point64::new(pt.x, pt.y));
    }

    let paths_to_offset = vec![path];

    clipper2_rust::inflate_paths_64(
        &paths_to_offset,
        width as f64 / 2.0,
        JoinType::Miter,
        EndType::Butt,
        2.0,
        0.0,
    )
}
