//! 2D Polygon Co-Unioning (Copper Welder) for PCB/APCB autorouter.
//!
//! Performs boolean union of copper polygons per layer/net using clipper2.
//! All coordinates use i64 nanometers. No f64 in core path.

use std::collections::HashMap;

use clipper2_rust::clipper::union_subjects_64;
use clipper2_rust::core::{FillRule, Path64, Paths64, Point64};

use crate::placement::PadShape;

/// A bucket grouping copper geometries by net and material.
#[derive(Debug, Clone)]
pub struct CopperBucket {
    pub net_id: u32,
    pub material_id: u8,
    pub polygons: Vec<Vec<(i64, i64)>>,
}

/// Welded copper layer result with separated contours and holes.
#[derive(Debug, Clone)]
pub struct WeldedLayer {
    pub net_id: u32,
    pub material_id: u8,
    pub contours: Vec<Vec<(i64, i64)>>,
    pub holes: Vec<Vec<(i64, i64)>>,
}

/// Convert `(i64, i64)` tuple slice to a clipper2 `Path64`.
#[inline]
fn to_clipper_path(pts: &[(i64, i64)]) -> Path64 {
    pts.iter().map(|&(x, y)| Point64 { x, y }).collect()
}

/// Convert clipper2 `Path64` back to `(i64, i64)` tuple vec.
#[inline]
fn from_clipper_path(path: &Path64) -> Vec<(i64, i64)> {
    path.iter().map(|p| (p.x, p.y)).collect()
}

/// Bucket input shapes by `(net_id, material_id)`.
pub fn bucket_copper(
    shapes: &[(u32, u8, Vec<Vec<(i64, i64)>>)],
) -> HashMap<(u32, u8), CopperBucket> {
    let mut map: HashMap<(u32, u8), CopperBucket> = HashMap::new();

    for &(net_id, material_id, ref polygons) in shapes {
        let key = (net_id, material_id);
        map.entry(key)
            .and_modify(|bucket| bucket.polygons.extend(polygons.clone()))
            .or_insert_with(|| CopperBucket {
                net_id,
                material_id,
                polygons: polygons.clone(),
            });
    }

    map
}

/// Convert a rectangle to a 4-point closed path (CCW winding).
///
/// CCW winding: bottom-left → bottom-right → top-right → top-left.
#[inline]
pub fn rect_to_path(x: i64, y: i64, width: i64, height: i64) -> Vec<(i64, i64)> {
    vec![
        (x, y),
        (x + width, y),
        (x + width, y + height),
        (x, y + height),
    ]
}

/// Convert a circle to a regular polygon approximation.
///
/// Default 64 segments for smooth rendering. Uses CCW winding.
#[inline]
pub fn circle_to_path(cx: i64, cy: i64, radius: i64, num_segments: usize) -> Vec<(i64, i64)> {
    if num_segments == 0 || radius <= 0 {
        return Vec::new();
    }

    let two_pi = core::f64::consts::TAU;
    let mut pts = Vec::with_capacity(num_segments);

    for i in 0..num_segments {
        let angle = two_pi * (i as f64) / (num_segments as f64);
        let x = cx + (radius as f64 * angle.cos()) as i64;
        let y = cy + (radius as f64 * angle.sin()) as i64;
        pts.push((x, y));
    }

    pts
}

/// Default number of segments for circle approximation.
const CIRCLE_SEGMENTS: usize = 64;

/// Convert a `PadShape` to a closed polygon path, translated by origin.
///
/// Returns CCW wound polygon relative to the given origin.
pub fn pad_shape_to_path(shape: &PadShape, origin_x: i64, origin_y: i64) -> Vec<(i64, i64)> {
    match shape {
        PadShape::Rectangle {
            width_nm,
            height_nm,
        } => {
            let hw = width_nm / 2;
            let hh = height_nm / 2;
            rect_to_path(origin_x - hw, origin_y - hh, *width_nm, *height_nm)
        }
        PadShape::Circle { diameter_nm } => {
            let radius = diameter_nm / 2;
            circle_to_path(origin_x, origin_y, radius, CIRCLE_SEGMENTS)
        }
        PadShape::Obround {
            width_nm,
            height_nm,
        } => {
            let hw = width_nm / 2;
            let hh = height_nm / 2;
            let radius = hw.min(hh);
            let mut pts = Vec::new();

            // Top edge (left to right)
            pts.push((origin_x - hw + radius, origin_y - hh));
            pts.push((origin_x + hw - radius, origin_y - hh));

            // Right arc
            let segments = CIRCLE_SEGMENTS / 4;
            for i in 0..=segments {
                let angle = -core::f64::consts::FRAC_PI_2
                    + core::f64::consts::PI * (i as f64) / (segments as f64);
                let x = origin_x + hw - radius + (radius as f64 * angle.cos()) as i64;
                let y = origin_y + (radius as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            // Bottom edge (right to left)
            pts.push((origin_x + hw - radius, origin_y + hh));
            pts.push((origin_x - hw + radius, origin_y + hh));

            // Left arc
            for i in 0..=segments {
                let angle = core::f64::consts::FRAC_PI_2
                    + core::f64::consts::PI * (i as f64) / (segments as f64);
                let x = origin_x - hw + radius + (radius as f64 * angle.cos()) as i64;
                let y = origin_y + (radius as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            pts
        }
        PadShape::Polygon { points } => {
            let mut pts = Vec::with_capacity(points.len());
            for p in points {
                pts.push((p.x + origin_x, p.y + origin_y));
            }
            pts
        }
        PadShape::RoundedRect {
            width_nm,
            height_nm,
            corner_radius_nm,
        } => {
            let hw = *width_nm / 2;
            let hh = *height_nm / 2;
            let r = (*corner_radius_nm).min(hw).min(hh);
            let segments = CIRCLE_SEGMENTS / 4;
            let mut pts = Vec::new();

            // Bottom-right corner
            for i in 0..=segments {
                let angle = -core::f64::consts::FRAC_PI_2
                    + core::f64::consts::FRAC_PI_2 * (i as f64) / (segments as f64);
                let x = origin_x + hw - r + (r as f64 * angle.cos()) as i64;
                let y = origin_y + hh - r + (r as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            // Top-right corner
            for i in 0..=segments {
                let angle = core::f64::consts::FRAC_PI_2 * (i as f64) / (segments as f64);
                let x = origin_x + hw - r + (r as f64 * angle.cos()) as i64;
                let y = origin_y - hh + r + (r as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            // Top-left corner
            for i in 0..=segments {
                let angle = core::f64::consts::PI
                    + core::f64::consts::FRAC_PI_2 * (i as f64) / (segments as f64);
                let x = origin_x - hw + r + (r as f64 * angle.cos()) as i64;
                let y = origin_y - hh + r + (r as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            // Bottom-left corner
            for i in 0..=segments {
                let angle = core::f64::consts::FRAC_PI_2
                    + core::f64::consts::FRAC_PI_2 * (i as f64) / (segments as f64);
                let x = origin_x - hw + r + (r as f64 * angle.cos()) as i64;
                let y = origin_y + hh - r + (r as f64 * angle.sin()) as i64;
                pts.push((x, y));
            }

            pts
        }
    }
}

/// Boolean union of polygons under Non-Zero Winding Rule.
///
/// Uses clipper2 for robust polygon union. Returns merged polygon contours.
/// Empty input returns empty output. Single polygon passes through unchanged.
pub fn union_polygons(polygons: Vec<Vec<(i64, i64)>>) -> Vec<Vec<(i64, i64)>> {
    if polygons.is_empty() {
        return Vec::new();
    }

    if polygons.len() == 1 {
        return polygons;
    }

    let subject_paths: Paths64 = polygons
        .iter()
        .map(|p| to_clipper_path(p))
        .filter(|p| p.len() >= 3)
        .collect();

    if subject_paths.is_empty() {
        return Vec::new();
    }

    let result = union_subjects_64(&subject_paths, FillRule::NonZero);

    result.iter().map(|p| from_clipper_path(p)).collect()
}

/// Complete copper weld pipeline for a layer.
///
/// Buckets shapes by (net_id, material_id), performs union per bucket,
/// separates outer contours from holes using winding direction.
pub fn weld_layer_copper(shapes: Vec<(u32, u8, Vec<Vec<(i64, i64)>>)>) -> Vec<WeldedLayer> {
    let buckets = bucket_copper(&shapes);

    buckets
        .into_values()
        .map(|bucket| {
            let merged = union_polygons(bucket.polygons);
            let mut contours = Vec::new();
            let mut holes = Vec::new();

            for polygon in &merged {
                if polygon.len() < 3 {
                    continue;
                }
                // CCW = outer contour (positive area), CW = hole (negative area)
                let area: i128 = polygon
                    .iter()
                    .enumerate()
                    .map(|(i, &(x, y))| {
                        let (x2, y2) = polygon[(i + 1) % polygon.len()];
                        (x as i128) * (y2 as i128) - (x2 as i128) * (y as i128)
                    })
                    .sum::<i128>()
                    / 2;
                if area > 0 {
                    contours.push(polygon.clone());
                } else {
                    holes.push(polygon.clone());
                }
            }

            WeldedLayer {
                net_id: bucket.net_id,
                material_id: bucket.material_id,
                contours,
                holes,
            }
        })
        .collect()
}

/// Verify that a contour boundary is clean (no self-intersections).
///
/// Uses a simple O(n²) edge intersection check. For production, clipper2's
/// own validation is more robust, but this catches obvious issues.
pub fn verify_no_self_intersections(contour: &[(i64, i64)]) -> bool {
    let n = contour.len();
    if n < 3 {
        return false;
    }

    // Check for duplicate consecutive points
    for i in 0..n {
        let next = (i + 1) % n;
        if contour[i] == contour[next] {
            return false;
        }
    }

    // Check for edge intersections (skip adjacent edges)
    for i in 0..n {
        let a1 = contour[i];
        let a2 = contour[(i + 1) % n];
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue; // Skip adjacent edges
            }
            let b1 = contour[j];
            let b2 = contour[(j + 1) % n];
            if segments_intersect(a1, a2, b1, b2) {
                return false;
            }
        }
    }

    true
}

/// Check if two line segments intersect (excluding shared endpoints).
#[inline]
fn segments_intersect(p1: (i64, i64), p2: (i64, i64), p3: (i64, i64), p4: (i64, i64)) -> bool {
    let d1 = cross_product(p3, p4, p1);
    let d2 = cross_product(p3, p4, p2);
    let d3 = cross_product(p1, p2, p3);
    let d4 = cross_product(p1, p2, p4);

    if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
        return true;
    }

    false
}

/// 2D cross product of vectors (p2-p1) and (p3-p2).
#[inline]
fn cross_product(p1: (i64, i64), p2: (i64, i64), p3: (i64, i64)) -> i64 {
    (p2.0 - p1.0) * (p3.1 - p2.1) - (p2.1 - p1.1) * (p3.0 - p2.0)
}

/// Compute bounding box of a contour.
///
/// Returns `(min_x, min_y, max_x, max_y)`.
pub fn bounding_box_of_contour(contour: &[(i64, i64)]) -> (i64, i64, i64, i64) {
    if contour.is_empty() {
        return (0, 0, 0, 0);
    }

    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;

    for &(x, y) in contour {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (min_x, min_y, max_x, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_to_path() {
        let path = rect_to_path(100, 200, 300, 400);
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], (100, 200));
        assert_eq!(path[1], (400, 200));
        assert_eq!(path[2], (400, 600));
        assert_eq!(path[3], (100, 600));
    }

    #[test]
    fn test_circle_to_path() {
        let path = circle_to_path(0, 0, 1000, 64);
        assert_eq!(path.len(), 64);

        // Verify all points are approximately radius distance from center
        for &(x, y) in &path {
            let dist_sq = (x as f64).powi(2) + (y as f64).powi(2);
            let dist = dist_sq.sqrt();
            assert!(
                (dist - 1000.0).abs() < 50.0,
                "Point ({}, {}) is at distance {} from origin, expected ~1000",
                x,
                y,
                dist
            );
        }
    }

    #[test]
    fn test_union_polygons_overlapping() {
        let rect1 = rect_to_path(0, 0, 200, 200);
        let rect2 = rect_to_path(100, 100, 200, 200);
        let result = union_polygons(vec![rect1, rect2]);

        // Two overlapping rectangles should produce a single merged contour
        assert_eq!(result.len(), 1);
        assert!(result[0].len() >= 4);
    }

    #[test]
    fn test_union_polygons_non_overlapping() {
        let rect1 = rect_to_path(0, 0, 100, 100);
        let rect2 = rect_to_path(500, 500, 100, 100);
        let result = union_polygons(vec![rect1, rect2]);

        // Non-overlapping rectangles should produce two separate contours
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_union_polygons_empty() {
        let result = union_polygons(Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_union_polygons_single() {
        let rect = rect_to_path(0, 0, 100, 100);
        let result = union_polygons(vec![rect.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], rect);
    }

    #[test]
    fn test_bucket_copper() {
        let shapes = vec![
            (1, 0, vec![rect_to_path(0, 0, 100, 100)]),
            (2, 0, vec![rect_to_path(200, 200, 100, 100)]),
            (1, 0, vec![rect_to_path(50, 50, 100, 100)]),
            (1, 1, vec![rect_to_path(300, 300, 100, 100)]),
        ];

        let buckets = bucket_copper(&shapes);
        assert_eq!(buckets.len(), 3);

        let bucket_1_0 = buckets.get(&(1, 0)).expect("bucket (1,0) missing");
        assert_eq!(bucket_1_0.polygons.len(), 2);

        let bucket_2_0 = buckets.get(&(2, 0)).expect("bucket (2,0) missing");
        assert_eq!(bucket_2_0.polygons.len(), 1);

        let bucket_1_1 = buckets.get(&(1, 1)).expect("bucket (1,1) missing");
        assert_eq!(bucket_1_1.polygons.len(), 1);
    }

    #[test]
    fn test_pad_shape_to_path_rect() {
        let shape = PadShape::Rectangle {
            width_nm: 200,
            height_nm: 100,
        };
        let path = pad_shape_to_path(&shape, 500, 500);
        assert_eq!(path.len(), 4);

        // Centered at (500,500), width=200, height=100
        // min_x = 500 - 100 = 400, min_y = 500 - 50 = 450
        assert_eq!(path[0], (400, 450));
        assert_eq!(path[1], (600, 450));
        assert_eq!(path[2], (600, 550));
        assert_eq!(path[3], (400, 550));
    }

    #[test]
    fn test_pad_shape_to_path_circle() {
        let shape = PadShape::Circle { diameter_nm: 200 };
        let path = pad_shape_to_path(&shape, 500, 500);
        assert_eq!(path.len(), CIRCLE_SEGMENTS);

        // All points should be roughly 100nm from center
        for &(x, y) in &path {
            let dx = x - 500;
            let dy = y - 500;
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            assert!(
                (dist - 100.0).abs() < 5.0,
                "Circle point ({}, {}) is at distance {}, expected ~100",
                x,
                y,
                dist
            );
        }
    }

    #[test]
    fn test_verify_no_self_intersections() {
        let clean_rect = rect_to_path(0, 0, 100, 100);
        assert!(verify_no_self_intersections(&clean_rect));

        // A figure-8 would self-intersect - create one manually
        let figure8 = vec![(0, 0), (100, 100), (100, 0), (0, 100)];
        assert!(!verify_no_self_intersections(&figure8));

        // Empty path
        assert!(!verify_no_self_intersections(&[]));
    }

    #[test]
    fn test_bounding_box_of_contour() {
        let contour = vec![(100, 200), (500, 200), (500, 800), (100, 800)];
        let (min_x, min_y, max_x, max_y) = bounding_box_of_contour(&contour);
        assert_eq!(min_x, 100);
        assert_eq!(min_y, 200);
        assert_eq!(max_x, 500);
        assert_eq!(max_y, 800);
    }

    #[test]
    fn test_weld_layer_copper() {
        let shapes = vec![
            (
                1,
                0,
                vec![
                    rect_to_path(0, 0, 200, 200),
                    rect_to_path(100, 100, 200, 200),
                ],
            ),
            (2, 0, vec![rect_to_path(500, 500, 100, 100)]),
        ];

        let result = weld_layer_copper(shapes);
        assert_eq!(result.len(), 2);

        let welded_1 = result
            .iter()
            .find(|w| w.net_id == 1 && w.material_id == 0)
            .expect("welded layer (1,0) missing");
        // Two overlapping rects → single merged contour
        assert_eq!(welded_1.contours.len(), 1);
        assert!(welded_1.holes.is_empty());

        let welded_2 = result
            .iter()
            .find(|w| w.net_id == 2 && w.material_id == 0)
            .expect("welded layer (2,0) missing");
        assert_eq!(welded_2.contours.len(), 1);
        assert!(welded_2.holes.is_empty());
    }
}
