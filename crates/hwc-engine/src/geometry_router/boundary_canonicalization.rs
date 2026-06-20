//! Boundary Canonicalization — cleans and normalises 2D polygons.
//!
//! Operations:
//! - Collinear edge merging
//! - Sliver removal (degenerate micro-loops)
//! - Winding normalisation (CCW for outer contours, CW for holes)
//! - Full canonicalization pipeline
//! - Signed area (shoelace, i128)
//! - Point-in-polygon (ray casting)

/// Winding convention for a polygon contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindingType {
    OuterContour,
    HoleContour,
}

// ---------------------------------------------------------------------------
// Signed area (shoelace, i128 intermediate)
// ---------------------------------------------------------------------------

/// Compute the signed area of a polygon via the shoelace formula.
///
/// Uses `i128` to avoid overflow on large polygons (coordinates in nanometers).
/// Positive → CCW, Negative → CW.
#[inline]
pub fn signed_area(polygon: &[(i64, i64)]) -> i128 {
    let n = polygon.len();
    if n < 3 {
        return 0;
    }
    let mut area: i128 = 0;
    for i in 0..n {
        let (x0, y0) = polygon[i];
        let (x1, y1) = polygon[(i + 1) % n];
        area += (x0 as i128) * (y1 as i128) - (x1 as i128) * (y0 as i128);
    }
    area / 2
}

// ---------------------------------------------------------------------------
// Collinear edge merging
// ---------------------------------------------------------------------------

/// Merge collinear edges, discarding vertices that lie on a straight line.
///
/// Tolerance: `|cross_product| < max(1, min(|AB|, |BC|) / 1000)`.
/// Adaptive — avoids dropping vertices on very short segments.
///
/// v0.1.8: Scalable resolution-aware tolerance. Capped to `resolution_nm`
/// to ensure physical correctness at the user-defined snap-step.
#[inline]
pub fn merge_collinear_edges(polygon: &[(i64, i64)], resolution_nm: i64) -> Vec<(i64, i64)> {
    let n = polygon.len();
    if n < 3 {
        return polygon.to_vec();
    }

    let mut result: Vec<(i64, i64)> = Vec::with_capacity(n);

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let curr = polygon[i];
        let next = polygon[(i + 1) % n];

        let abx = curr.0 - prev.0;
        let aby = curr.1 - prev.1;
        let bcx = next.0 - curr.0;
        let bcy = next.1 - curr.1;

        let cross = abx * bcy - aby * bcx;

        let len_ab = abx.abs().max(aby.abs());
        let len_bc = bcx.abs().max(bcy.abs());
        
        // v0.1.8: Scalable 'Pad Deformation' protection.
        // Instead of a hardcoded 1000nm (1um) cap, we scale the tolerance 
        // relative to the user-defined resolution snap-step.
        // This ensures sub-atomic physical correctness at any scale.
        let tolerance = 1_i64.max((len_ab.min(len_bc) / 1000).min(resolution_nm));

        if cross.abs() >= tolerance {
            result.push(curr);
        }
    }

    if result.len() < 3 {
        return polygon.to_vec();
    }

    result
}

// ---------------------------------------------------------------------------
// Sliver removal
// ---------------------------------------------------------------------------

/// Remove sliver polygons whose absolute signed area is below `min_area`.
#[inline]
pub fn remove_slivers(polygon: &[(i64, i64)], min_area: i64) -> Vec<(i64, i64)> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    let area = signed_area(polygon);
    if area.abs() < min_area as i128 {
        Vec::new()
    } else {
        polygon.to_vec()
    }
}

/// Batch sliver removal.
pub fn clean_polygons(polygons: Vec<Vec<(i64, i64)>>, min_area: i64) -> Vec<Vec<(i64, i64)>> {
    polygons
        .into_iter()
        .filter_map(|p| {
            let cleaned = remove_slivers(&p, min_area);
            if cleaned.is_empty() { None } else { Some(cleaned) }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Winding normalisation
// ---------------------------------------------------------------------------

/// Normalise winding so that an outer contour is CCW (positive signed area).
///
/// If the polygon is CW (negative signed area) it is reversed.
#[inline]
pub fn normalize_winding(polygon: &[(i64, i64)]) -> Vec<(i64, i64)> {
    if polygon.len() < 3 {
        return polygon.to_vec();
    }
    let area = signed_area(polygon);
    if area < 0 {
        polygon.iter().rev().copied().collect()
    } else {
        polygon.to_vec()
    }
}

/// Ensure all hole contours are CW (negative signed area).
#[inline]
pub fn normalize_holes(holes: &mut Vec<Vec<(i64, i64)>>) {
    for hole in holes.iter_mut() {
        if hole.len() < 3 {
            continue;
        }
        let area = signed_area(hole);
        if area > 0 {
            hole.reverse();
        }
    }
}

// ---------------------------------------------------------------------------
// Point-in-polygon (ray casting)
// ---------------------------------------------------------------------------

/// Test whether `point` lies inside `polygon` using the ray casting algorithm.
///
/// Returns `true` for points on edges as well.
#[inline]
pub fn point_in_polygon(point: (i64, i64), polygon: &[(i64, i64)]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }

    let (px, py) = point;
    let mut inside = false;

    for i in 0..n {
        let (x0, y0) = polygon[i];
        let (x1, y1) = polygon[(i + 1) % n];

        if point_on_segment(px, py, x0, y0, x1, y1) {
            return true;
        }

        let crosses = ((y0 > py) != (y1 > py))
            && (px < (x1 - x0) * (py - y0) / (y1 - y0) + x0);
        if crosses {
            inside = !inside;
        }
    }

    inside
}

/// Test if (px, py) lies on the line segment (x0,y0)-(x1,y1).
#[inline]
fn point_on_segment(px: i64, py: i64, x0: i64, y0: i64, x1: i64, y1: i64) -> bool {
    let min_x = x0.min(x1);
    let max_x = x0.max(x1);
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);
    if px < min_x || px > max_x || py < min_y || py > max_y {
        return false;
    }
    let cross = ((px - x0) as i128) * ((y1 - y0) as i128)
        - ((py - y0) as i128) * ((x1 - x0) as i128);
    cross == 0
}

// ---------------------------------------------------------------------------
// Full canonicalization pipeline
// ---------------------------------------------------------------------------

/// Run the full canonicalization pipeline on a single polygon.
///
/// Pipeline: merge collinear → remove slivers → normalise winding.
///
/// Returns `None` if the polygon is degenerate (< 3 vertices after merging
/// or zero/underflow area).
#[inline]
pub fn canonicalize(
    polygon: Vec<(i64, i64)>,
    winding: WindingType,
    min_area: i64,
    resolution_nm: i64,
) -> Option<Vec<(i64, i64)>> {
    if polygon.len() < 3 {
        return None;
    }

    let merged = merge_collinear_edges(&polygon, resolution_nm);
    if merged.len() < 3 {
        return None;
    }

    let cleaned = remove_slivers(&merged, min_area);
    if cleaned.len() < 3 {
        return None;
    }

    let normalised = match winding {
        WindingType::OuterContour => normalize_winding(&cleaned),
        WindingType::HoleContour => {
            let mut h = cleaned;
            if signed_area(&h) > 0 {
                h.reverse();
            }
            h
        }
    };

    if signed_area(&normalised) == 0 {
        return None;
    }

    Some(normalised)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Collinear merging
    // -----------------------------------------------------------------------

    #[test]
    fn collinear_merge_reduces_vertices_on_straight_edges() {
        let polygon = vec![
            (0, 0),
            (100, 0),
            (200, 0),
            (300, 0),
            (300, 300),
            (0, 300),
        ];
        let merged = merge_collinear_edges(&polygon, 1000);
        assert!(merged.len() < polygon.len());
        assert!(merged.len() >= 3);
        assert!(merged.contains(&(0, 0)));
        assert!(merged.contains(&(300, 300)));
    }

    #[test]
    fn collinear_merge_keeps_corners() {
        let polygon = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        let merged = merge_collinear_edges(&polygon, 1000);
        assert_eq!(merged, polygon);
    }

    // -----------------------------------------------------------------------
    // Sliver removal
    // -----------------------------------------------------------------------

    #[test]
    fn sliver_removal_eliminates_tiny_polygon() {
        let polygon = vec![(0, 0), (1, 0), (0, 1)];
        let cleaned = remove_slivers(&polygon, 10);
        assert!(cleaned.is_empty());
    }

    #[test]
    fn sliver_removal_keeps_large_polygon() {
        let polygon = vec![(0, 0), (1000, 0), (0, 1000)];
        let cleaned = remove_slivers(&polygon, 100);
        assert_eq!(cleaned.len(), 3);
    }

    #[test]
    fn clean_polygons_batch() {
        let small = vec![(0, 0), (1, 0), (0, 1)];
        let large = vec![(0, 0), (1000, 0), (0, 1000)];
        let result = clean_polygons(vec![small, large], 10);
        assert_eq!(result.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Signed area
    // -----------------------------------------------------------------------

    #[test]
    fn signed_area_ccw_positive() {
        let polygon = vec![(0, 0), (10, 0), (0, 10)];
        assert!(signed_area(&polygon) > 0);
        assert_eq!(signed_area(&polygon), 50);
    }

    #[test]
    fn signed_area_cw_negative() {
        let polygon = vec![(0, 0), (0, 10), (10, 0)];
        assert!(signed_area(&polygon) < 0);
        assert_eq!(signed_area(&polygon), -50);
    }

    #[test]
    fn signed_area_degenerate() {
        let polygon = vec![(0, 0), (1, 1)];
        assert_eq!(signed_area(&polygon), 0);
    }

    // -----------------------------------------------------------------------
    // Winding normalisation
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_winding_cw_outer_to_ccw() {
        let polygon = vec![(0, 0), (0, 100), (100, 100), (100, 0)];
        let area_before = signed_area(&polygon);
        assert!(area_before < 0, "expected CW before normalisation");

        let normalised = normalize_winding(&polygon);
        let area_after = signed_area(&normalised);
        assert!(area_after > 0, "expected CCW after normalisation");
    }

    #[test]
    fn normalize_winding_ccw_stays_ccw() {
        let polygon = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        let normalised = normalize_winding(&polygon);
        assert!(signed_area(&normalised) > 0);
    }

    #[test]
    fn normalize_holes_makes_cw() {
        let mut holes: Vec<Vec<(i64, i64)>> = vec![
            vec![(10, 10), (10, 90), (90, 90), (90, 10)],
            vec![(20, 20), (20, 80), (80, 80), (80, 20)],
        ];
        normalize_holes(&mut holes);
        for hole in &holes {
            let area = signed_area(hole);
            assert!(area <= 0, "hole should be CW (area={area})");
        }
    }

    // -----------------------------------------------------------------------
    // Point-in-polygon
    // -----------------------------------------------------------------------

    #[test]
    fn point_in_polygon_inside() {
        let polygon = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        assert!(point_in_polygon((50, 50), &polygon));
    }

    #[test]
    fn point_in_polygon_outside() {
        let polygon = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        assert!(!point_in_polygon((200, 200), &polygon));
    }

    #[test]
    fn point_in_polygon_on_edge() {
        let polygon = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        assert!(point_in_polygon((50, 0), &polygon));
    }

    // -----------------------------------------------------------------------
    // Full canonicalization pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn canonicalize_polygon_with_collinear_edges_and_slivers() {
        let polygon = vec![
            (0, 0),
            (50, 0),
            (100, 0),
            (100, 100),
            (0, 100),
        ];
        let result = canonicalize(polygon, WindingType::OuterContour, 10);
        let result = result.expect("should produce a valid polygon");
        assert!(result.len() >= 3);
        assert!(signed_area(&result) > 0, "outer contour should be CCW");
    }

    #[test]
    fn canonicalize_degenerate_sliver_returns_none() {
        let polygon = vec![(0, 0), (1, 0), (0, 1)];
        assert!(canonicalize(polygon, WindingType::OuterContour, 100).is_none());
    }

    #[test]
    fn canonicalize_hole_produces_cw() {
        let polygon = vec![
            (10, 10),
            (90, 10),
            (90, 90),
            (10, 90),
        ];
        let result = canonicalize(polygon, WindingType::HoleContour, 10);
        let result = result.expect("should produce a valid polygon");
        assert!(signed_area(&result) < 0, "hole contour should be CW");
    }
}
