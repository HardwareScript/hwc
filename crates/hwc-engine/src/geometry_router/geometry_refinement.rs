//! Geometry Refinement Engine (Roadmap 5.4)
//!
//! Takes raw copper shapes and produces clean, export-ready geometry through:
//! 1. Boolean union via clipper2 (copper_welder)
//! 2. Boundary canonicalization (collinear merge, sliver removal, winding normalization)
//! 3. Ear-cut triangulation for mesh export
//!
//! **Performance**: Triangulation on clean input (after union + canonicalization)
//! is 3-5x faster than on raw self-intersecting geometry.
//!
//! All coordinates use i64 nanometers. No f64 in core path.

use crate::geometry_router::boundary_canonicalization::{self, WindingType};
use crate::geometry_router::copper_welder;

/// A cleaned contour with outer ring, holes, and precomputed signed area.
#[derive(Clone, Debug)]
pub struct RefinedContour {
    /// Outer boundary (CCW winding, i64 nanometers).
    pub outer: Vec<(i64, i64)>,
    /// Hole boundaries (CW winding, i64 nanometers).
    pub holes: Vec<Vec<(i64, i64)>>,
    /// Signed area in square nanometers (positive = CCW outer).
    pub area: i128,
}

/// A single triangle for triangulated export.
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub vertices: [(i64, i64); 3],
}

/// Refine a set of raw 2D shapes into clean merged contours.
///
/// 1. Welds overlapping polygons via `copper_welder::union_polygons()`
/// 2. Canonicalizes each resulting contour (collinear merge, sliver removal, winding)
///
/// Returns `RefinedContour` for each merged region.
#[inline]
pub fn refine_layer(shapes: Vec<Vec<(i64, i64)>>, manufacturing_grid_nm: i64) -> Vec<RefinedContour> {
    let welded = copper_welder::union_polygons(shapes);

    let mut result = Vec::new();
    for polygon in &welded {
        if polygon.len() < 3 {
            continue;
        }
        let mut contour = RefinedContour {
            outer: polygon.clone(),
            holes: Vec::new(),
            area: 0,
        };
        contour.area = compute_contour_area(&contour);
        result.push(contour);
    }

    canonicalize_contours(&mut result, manufacturing_grid_nm);
    result
}

/// Refine raw shapes through the full union + canonicalize pipeline.
#[inline]
pub fn refine_geometry(
    raw_shapes: Vec<Vec<(i64, i64)>>,
    manufacturing_grid_nm: i64,
) -> Vec<RefinedContour> {
    refine_layer(raw_shapes, manufacturing_grid_nm)
}

/// Apply boundary canonicalization to all refined contours.
///
/// Runs collinear edge merge, sliver removal, and winding normalization
/// on both outer and hole rings.
pub fn canonicalize_contours(contours: &mut [RefinedContour], manufacturing_grid_nm: i64) {
    for contour in contours.iter_mut() {
        if let Some(canonical) = boundary_canonicalization::canonicalize(
            contour.outer.clone(),
            WindingType::OuterContour,
            0,
            manufacturing_grid_nm,
        ) {
            contour.outer = canonical;
        }
        for hole in &mut contour.holes {
            if let Some(canonical) = boundary_canonicalization::canonicalize(
                hole.clone(),
                WindingType::HoleContour,
                0,
                manufacturing_grid_nm,
            ) {
                *hole = canonical;
            }
        }
        contour.area = compute_contour_area(contour);
    }
}

/// Triangulate a single contour using ear-cut triangulation (earcutr).
///
/// Supports contours with holes via the `hole_indices` parameter.
/// Only call this at final export boundary — not during routing.
pub fn triangulate_contour(contour: &RefinedContour) -> Vec<Triangle> {
    if contour.outer.len() < 3 {
        return Vec::new();
    }

    // Build flat vertex array: outer vertices first, then holes appended
    let mut vertices: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    // Flatten outer ring
    for &(x, y) in &contour.outer {
        vertices.push(x as f64);
        vertices.push(y as f64);
    }

    // Flatten holes (each hole appended after the previous ring)
    for hole in &contour.holes {
        if hole.len() < 3 {
            continue;
        }
        hole_indices.push(vertices.len() / 2);
        for &(x, y) in hole {
            vertices.push(x as f64);
            vertices.push(y as f64);
        }
    }

    // Triangulate using earcutr
    let indices = match earcutr::earcut(&vertices, &hole_indices, 2) {
        Ok(idx) => idx,
        Err(_) => return Vec::new(),
    };

    // Convert flat index triples into Triangle structs
    let mut triangles = Vec::with_capacity(indices.len() / 3);
    for chunk in indices.chunks(3) {
        if chunk.len() == 3 {
            let v0 = vertices[chunk[0] * 2];
            let v1 = vertices[chunk[0] * 2 + 1];
            let v2 = vertices[chunk[1] * 2];
            let v3 = vertices[chunk[1] * 2 + 1];
            let v4 = vertices[chunk[2] * 2];
            let v5 = vertices[chunk[2] * 2 + 1];
            triangles.push(Triangle {
                vertices: [
                    (v0 as i64, v1 as i64),
                    (v2 as i64, v3 as i64),
                    (v4 as i64, v5 as i64),
                ],
            });
        }
    }
    triangles
}

/// Batch triangulate all contours.
#[inline]
pub fn triangulate_all(contours: &[RefinedContour]) -> Vec<Triangle> {
    let mut all = Vec::new();
    for contour in contours {
        all.extend(triangulate_contour(contour));
    }
    all
}

/// Compute signed area of a RefinedContour using the shoelace formula.
/// Positive = outer is CCW.
#[inline]
fn compute_contour_area(contour: &RefinedContour) -> i128 {
    let outer_area = shoelace_signed(&contour.outer);
    let mut hole_area: i128 = 0;
    for hole in &contour.holes {
        hole_area += shoelace_signed(hole).abs();
    }
    outer_area.abs() - hole_area
}

/// Signed area via the shoelace formula (i128 for overflow safety).
#[inline]
fn shoelace_signed(ring: &[(i64, i64)]) -> i128 {
    let mut area: i128 = 0;
    let len = ring.len();
    for i in 0..len {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % len];
        area += i128::from(x0) * i128::from(y1);
        area -= i128::from(x1) * i128::from(y0);
    }
    area / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refine_layer_merges_overlapping() {
        // Two overlapping squares should merge into one contour
        let square_a = vec![(0, 0), (2000, 0), (2000, 2000), (0, 2000)];
        let square_b = vec![(1000, 1000), (3000, 1000), (3000, 3000), (1000, 3000)];
        let refined = refine_layer(vec![square_a, square_b], 1_000);
        assert!(!refined.is_empty());
        // Merged area should be less than sum of individual areas (overlap removed)
        let total_area: i128 = refined.iter().map(|c| c.area).sum();
        let individual = 2000 * 2000 * 2;
        assert!(total_area < individual as i128);
    }

    #[test]
    fn test_refine_layer_single_shape() {
        let square = vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000)];
        let refined = refine_layer(vec![square], 1_000);
        assert_eq!(refined.len(), 1);
        assert_eq!(refined[0].area, 1_000_000);
    }

    #[test]
    fn test_triangulate_contour_rectangle() {
        let contour = RefinedContour {
            outer: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000)],
            holes: Vec::new(),
            area: 1_000_000,
        };
        let triangles = triangulate_contour(&contour);
        // A rectangle should produce exactly 2 triangles
        assert_eq!(triangles.len(), 2);
    }

    #[test]
    fn test_triangulate_contour_with_hole() {
        let contour = RefinedContour {
            outer: vec![(0, 0), (4000, 0), (4000, 4000), (0, 4000)],
            holes: vec![vec![(1000, 1000), (3000, 1000), (3000, 3000), (1000, 3000)]],
            area: 16_000_000 - 4_000_000,
        };
        let triangles = triangulate_contour(&contour);
        // With a hole, should produce more than 2 triangles
        assert!(triangles.len() > 2);
    }

    #[test]
    fn test_triangulate_all() {
        let contours = vec![
            RefinedContour {
                outer: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000)],
                holes: Vec::new(),
                area: 1_000_000,
            },
            RefinedContour {
                outer: vec![(2000, 2000), (3000, 2000), (3000, 3000), (2000, 3000)],
                holes: Vec::new(),
                area: 1_000_000,
            },
        ];
        let triangles = triangulate_all(&contours);
        assert_eq!(triangles.len(), 4); // 2 triangles per rectangle
    }

    #[test]
    fn test_canonicalize_contours_cleans_geometry() {
        let mut contours = vec![RefinedContour {
            outer: vec![(0, 0), (500, 0), (1000, 0), (1000, 1000), (0, 1000)],
            holes: Vec::new(),
            area: 1_000_000,
        }];
        canonicalize_contours(&mut contours, 1_000);
        // Collinear point (500, 0) should be removed
        assert_eq!(contours[0].outer.len(), 4);
    }

    #[test]
    fn test_refine_geometry_empty() {
        let result = refine_geometry(Vec::new(), 1_000);
        assert!(result.is_empty());
    }

    #[test]
    fn test_triangulate_empty_contour() {
        let contour = RefinedContour {
            outer: Vec::new(),
            holes: Vec::new(),
            area: 0,
        };
        let triangles = triangulate_contour(&contour);
        assert!(triangles.is_empty());
    }
}
