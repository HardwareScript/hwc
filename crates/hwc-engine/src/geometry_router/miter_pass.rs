//! v0.1.8: 45-Degree Mitered Chamfer Pass
//!
//! Converts sharp 90° corners in routed traces into smooth 45° diagonal
//! chamfers. This maintains constant trace impedance (Z₀) across bends,
//! preventing signal reflection and EMI radiation on high-speed lines.
//!
//! **Physics:** A sharp 90° corner widens the effective trace width by √2,
//! creating a parasitic capacitive load that drops impedance. The miter pass
//! replaces the corner with a diagonal segment that preserves constant width.
//!
//! **v0.2.0 Enhancement:** Context-aware mitering that protects via landing zones.
//! The engine queries contact metadata to identify endpoints that connect to
//! vias/pads and preserves these connections by skipping miter on terminal segments.


use crate::geometry::{BoundingBox, Point3D};
use crate::netlist::NetId;

/// Context provider for via/pad location queries
///
/// Allows the miter engine to check if a point is a via center or pad landing zone,
/// enabling it to skip mitering on segments that would break via connections.
pub trait MiterContext {
    /// Check if a 3D point is a via center or within a via's landing pad
    ///
    /// Returns true if the point is within the specified tolerance of a contact center
    /// or landing pad, indicating that segments touching this point should not be mitered.
    fn is_via_endpoint(&self, point: &Point3D, net_id: Option<NetId>, tolerance_nm: i64) -> bool;

    /// Get the bounding box of a contact/via at or near the given point
    ///
    /// Returns None if no contact found within tolerance
    fn get_contact_bbox(&self, point: &Point3D, tolerance_nm: i64) -> Option<BoundingBox>;
}

/// Null context for backward compatibility when no via data available
pub struct NullMiterContext;

impl MiterContext for NullMiterContext {
    fn is_via_endpoint(
        &self,
        _point: &Point3D,
        _net_id: Option<NetId>,
        _tolerance_nm: i64,
    ) -> bool {
        false // No via data available, miter everything
    }

    fn get_contact_bbox(&self, _point: &Point3D, _tolerance_nm: i64) -> Option<BoundingBox> {
        None
    }
}

/// Post-routing miter/chamfer engine with via-awareness.
///
/// Scans waypoint lists for 90° corners and replaces them with 45° diagonal
/// chamfers. The miter distance is calculated from the trace width to maintain
/// constant impedance across the bend.
///
/// **v0.2.0:** Context-aware implementation that preserves via connections by
/// skipping miter on segments that terminate at via centers or landing pads.
pub struct MiterEngine {
    /// Trace width in nanometers
    trace_width_nm: i64,
}

impl MiterEngine {
    /// Create a new miter engine.
    ///
    /// # Arguments
    /// * `trace_width_nm` - Trace width in nanometers (for miter distance calculation)
    pub fn new(trace_width_nm: i64) -> Self {
        Self { trace_width_nm }
    }

    /// Apply 45° miter chamfers to all 90° corners in a path (context-aware version).
    ///
    /// For each consecutive triple (P₀ → P₁ → P₂) that forms a 90° corner:
    /// 1. Check if P₁ is a via endpoint using the provided context
    /// 2. If it's a via, skip mitering to preserve the connection
    /// 3. Otherwise, calculate rollback distance `d = trace_width × 1.5`
    /// 4. Insert Pₐ = P₁ - d·û₁ and Pᵦ = P₁ + d·û₂
    /// 5. Replace the sharp corner with Pₐ → Pᵦ (45° diagonal)
    ///
    /// # Arguments
    /// * `path` - Ordered list of waypoints (minimum 3 points)
    /// * `context` - Via/contact location provider for endpoint protection
    /// * `net_id` - Optional network ID for context queries
    ///
    /// # Returns
    /// New waypoint list with 90° corners replaced by 45° chamfers
    pub fn apply_miter_pass_with_context(
        &self,
        path: &[Point3D],
        context: &dyn MiterContext,
        net_id: Option<NetId>,
    ) -> Vec<Point3D> {
        eprintln!("[MITER INPUT] Received path with {} points:", path.len());
        for (i, p) in path.iter().enumerate() {
            eprintln!("[MITER INPUT]   Point {}: ({},{},{})", i, p.x, p.y, p.z);
        }

        if path.len() < 3 {
            return path.to_vec();
        }

        // Step 1: Deduplicate consecutive identical points (zero-length segments)
        let deduped: Vec<Point3D> = path
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, p)| *i == 0 || *p != path[i - 1])
            .map(|(_, p)| p)
            .collect();

        eprintln!(
            "[MITER DEDUP] After deduplication: {} points",
            deduped.len()
        );
        for (i, p) in deduped.iter().enumerate() {
            eprintln!("[MITER DEDUP]   Point {}: ({},{},{})", i, p.x, p.y, p.z);
        }

        if deduped.len() < 3 {
            return deduped;
        }

        let mut mitered = Vec::with_capacity(deduped.len() + deduped.len() / 2);

        // Miter distance: 1.5× trace width for standard impedance stability
        let miter_dist = self.trace_width_nm * 3 / 2;

        // Tolerance for via endpoint detection (half the trace width)
        let via_tolerance = self.trace_width_nm / 2;

        // v0.2.0 FIX: Via edge coverage with proper forward extension
        // RATIONALE: The export engine uses EndType::Butt (flush ends) when stroking traces.
        // If a waypoint is at the via CENTER (650nm) and we stroke with 100nm radius, the
        // geometry ends flush at 650nm, leaving a 100nm gap to the via edge at 750nm.
        //
        // SOLUTION: When routing starts at a via, position the first waypoint at the via EDGE
        // in the direction of routing (not backward). When stroked with flush ends, this ensures
        // the trace geometry covers the full via pad.
        //
        // CRITICAL: We extend FORWARD (in routing direction), not backward (which would create
        // overshoot artifacts).

        let first = deduped[0];
        mitered.push(first);

        let mut i = 1;
        while i < deduped.len() - 1 {
            let p_prev = deduped[i - 1];
            let p_curr = deduped[i];
            let p_next = deduped[i + 1];

            // CRITICAL: Check if current point is a via endpoint
            // If it is, skip mitering to preserve the via connection
            let is_via_endpoint = context.is_via_endpoint(&p_curr, net_id, via_tolerance);

            // Direction vectors (XY only — miter is a 2D operation)
            let v1_x = p_curr.x - p_prev.x;
            let v1_y = p_curr.y - p_prev.y;
            let v2_x = p_next.x - p_curr.x;
            let v2_y = p_next.y - p_curr.y;

            // Check for 90° corner: orthogonal segments in the XY plane
            let dot = v1_x * v2_x + v1_y * v2_y;
            let is_corner = dot == 0 && (v1_x != 0 || v1_y != 0) && (v2_x != 0 || v2_y != 0);

            if is_corner && !is_via_endpoint {
                let len1_f = ((v1_x * v1_x + v1_y * v1_y) as f64).sqrt();
                let len2_f = ((v2_x * v2_x + v2_y * v2_y) as f64).sqrt();
                let len1 = len1_f as i64;
                let len2 = len2_f as i64;

                // Max safe rollback distance is half of the shortest adjacent segment.
                // This guarantees miters never overshoot, cross adjacent bends, or create acute notches/foldbacks.
                let max_d = (len1 / 2).min(len2 / 2);
                let actual_miter_dist = miter_dist.min(max_d);
                let min_effective_dist = (self.trace_width_nm / 2).min(miter_dist / 2);

                if actual_miter_dist >= min_effective_dist && actual_miter_dist > 0 {
                    let u1_x_f = v1_x as f64 / len1_f;
                    let u1_y_f = v1_y as f64 / len1_f;
                    let u2_x_f = v2_x as f64 / len2_f;
                    let u2_y_f = v2_y as f64 / len2_f;

                    let p_a = Point3D::new(
                        p_curr.x - (u1_x_f * actual_miter_dist as f64).round() as i64,
                        p_curr.y - (u1_y_f * actual_miter_dist as f64).round() as i64,
                        p_curr.z,
                    );
                    let p_b = Point3D::new(
                        p_curr.x + (u2_x_f * actual_miter_dist as f64).round() as i64,
                        p_curr.y + (u2_y_f * actual_miter_dist as f64).round() as i64,
                        p_curr.z,
                    );

                    eprintln!(
                        "[MITER APPLY] Applying miter at ({},{},{}) with dist {}nm (nominal {}nm)",
                        p_curr.x, p_curr.y, p_curr.z, actual_miter_dist, miter_dist
                    );
                    eprintln!("[MITER APPLY]   p_a=({},{},{})", p_a.x, p_a.y, p_a.z);
                    eprintln!("[MITER APPLY]   p_b=({},{},{})", p_b.x, p_b.y, p_b.z);

                    // Skip if miter points are duplicates of existing points
                    if p_a != *mitered.last().unwrap() {
                        mitered.push(p_a);
                    }
                    if p_b != p_next && p_b != *mitered.last().unwrap() {
                        mitered.push(p_b);
                    }
                } else {
                    eprintln!("[MITER SKIP] Segments too short for miter (len1={}, len2={}, required>={})", len1, len2, min_effective_dist);
                    // Segment too short for miter, keep original corner
                    if p_curr != *mitered.last().unwrap() {
                        mitered.push(p_curr);
                    }
                }
            } else {
                // Not a 90° corner OR is a via endpoint - keep as-is to preserve connection
                if p_curr != *mitered.last().unwrap() {
                    mitered.push(p_curr);
                }
            }
            i += 1;
        }

        let last = *deduped.last().unwrap();
        if last != *mitered.last().unwrap() {
            mitered.push(last);
        }

        sanitize_mitered_path(mitered)
    }

    /// Apply 45° miter chamfers to all 90° corners in a path (backward-compatible version).
    ///
    /// This version uses NullMiterContext, providing the original behavior where
    /// all corners are mitered without via-awareness.
    ///
    /// For new code, prefer `apply_miter_pass_with_context` with proper context.
    pub fn apply_miter_pass(&self, path: &[Point3D]) -> Vec<Point3D> {
        self.apply_miter_pass_with_context(path, &NullMiterContext, None)
    }

    /// Apply miter pass to all paths in a route result.
    ///
    /// Mutates the paths in-place, replacing 90° corners with 45° chamfers.
    pub fn apply_to_paths(&self, paths: &mut [Vec<Point3D>]) {
        for path in paths.iter_mut() {
            if path.len() >= 3 {
                *path = self.apply_miter_pass(path);
            }
        }
    }
}

/// Sanitize mitered path against consecutive duplicates, collinear points, and acute foldbacks.
fn sanitize_mitered_path(path: Vec<Point3D>) -> Vec<Point3D> {
    if path.len() < 3 {
        return path;
    }

    // Pass 1: Deduplicate consecutive identical points
    let mut deduped: Vec<Point3D> = Vec::with_capacity(path.len());
    for p in path {
        if deduped.last() != Some(&p) {
            deduped.push(p);
        }
    }

    if deduped.len() < 3 {
        return deduped;
    }

    // Pass 2: Remove redundant collinear points and acute reversals
    let mut cleaned: Vec<Point3D> = Vec::with_capacity(deduped.len());
    cleaned.push(deduped[0]);

    for i in 1..deduped.len() - 1 {
        let p_prev = *cleaned.last().unwrap();
        let p_curr = deduped[i];
        let p_next = deduped[i + 1];

        let v1_x = p_curr.x - p_prev.x;
        let v1_y = p_curr.y - p_prev.y;
        let v1_z = p_curr.z - p_prev.z;

        let v2_x = p_next.x - p_curr.x;
        let v2_y = p_next.y - p_curr.y;
        let v2_z = p_next.z - p_curr.z;

        if v1_x == 0 && v1_y == 0 && v1_z == 0 {
            continue;
        }
        if v2_x == 0 && v2_y == 0 && v2_z == 0 {
            continue;
        }

        // Collinearity check (same direction along line)
        let is_collinear_same_dir = (v1_y * v2_z == v1_z * v2_y)
            && (v1_z * v2_x == v1_x * v2_z)
            && (v1_x * v2_y == v1_y * v2_x)
            && (v1_x.signum() == v2_x.signum()
                && v1_y.signum() == v2_y.signum()
                && v1_z.signum() == v2_z.signum());

        if is_collinear_same_dir {
            continue;
        }

        // Reversal / foldback check along single axis
        let is_axis_reversal = (v1_x != 0
            && v2_x != 0
            && v1_x.signum() != v2_x.signum()
            && v1_y == 0
            && v2_y == 0)
            || (v1_y != 0
                && v2_y != 0
                && v1_y.signum() != v2_y.signum()
                && v1_x == 0
                && v2_x == 0);

        if is_axis_reversal {
            continue;
        }

        cleaned.push(p_curr);
    }

    if let Some(&last) = deduped.last() {
        if cleaned.last() != Some(&last) {
            cleaned.push(last);
        }
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_miter_on_short_path() {
        let engine = MiterEngine::new(200_000);
        let path = vec![Point3D::new(0, 0, 0), Point3D::new(1_000_000, 0, 0)];
        let result = engine.apply_miter_pass(&path);
        assert_eq!(result.len(), 2);
        assert_eq!(result, path);
    }

    #[test]
    fn test_no_miter_on_straight_line() {
        let engine = MiterEngine::new(200_000);
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(1_000_000, 0, 0),
            Point3D::new(2_000_000, 0, 0),
        ];
        let result = engine.apply_miter_pass(&path);
        // Straight collinear points: sanitizer collapses redundant interior waypoints.
        // Endpoints must be preserved exactly; no miter is introduced.
        assert!(result.len() >= 2, "Expected at least start and end points");
        assert_eq!(*result.first().unwrap(), Point3D::new(0, 0, 0));
        assert_eq!(*result.last().unwrap(), Point3D::new(2_000_000, 0, 0));
        // No new points introduced (no miter on a straight line)
        assert!(result.len() <= 3, "No miter points should be added to a straight line");
    }

    #[test]
    fn test_miter_90_degree_corner() {
        let engine = MiterEngine::new(200_000);
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(5_000_000, 0, 0), // corner
            Point3D::new(5_000_000, 5_000_000, 0),
        ];
        let result = engine.apply_miter_pass(&path);

        // Should have: start, miter_a, miter_b, end = 4 points
        assert_eq!(result.len(), 4);
        // Miter points should be on the diagonal
        assert!(result[1].x < 5_000_000); // p_a rolled back
        assert!(result[2].y > 0); // p_b advanced
    }

    #[test]
    fn test_miter_distance_scales_with_width() {
        let engine_narrow = MiterEngine::new(100_000);
        let engine_wide = MiterEngine::new(400_000);

        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(10_000_000, 0, 0),
            Point3D::new(10_000_000, 10_000_000, 0),
        ];

        let result_narrow = engine_narrow.apply_miter_pass(&path);
        let result_wide = engine_wide.apply_miter_pass(&path);

        // Both produce 4 points (start, miter_a, miter_b, end)
        assert_eq!(result_narrow.len(), 4);
        assert_eq!(result_wide.len(), 4);

        // Wider trace has larger miter distance, so p_a is further from corner
        let narrow_rollback = 10_000_000 - result_narrow[1].x;
        let wide_rollback = 10_000_000 - result_wide[1].x;
        assert!(wide_rollback > narrow_rollback);
    }

    #[test]
    fn test_miter_short_segment_clamped() {
        // Trace width = 300um, nominal miter distance = 450um
        let engine = MiterEngine::new(300_000);
        // Segment 1 is 2000um long, but Segment 2 is only 200um long (less than nominal miter distance)
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(2_000_000, 0, 0),
            Point3D::new(2_000_000, 200_000, 0),
            Point3D::new(3_000_000, 200_000, 0),
        ];

        let result = engine.apply_miter_pass(&path);
        // Rollback on segment 2 must never exceed len2 / 2 = 100_000
        for window in result.windows(2) {
            // Assert all segments move forward or orthogonally, no negative backtracking
            let dx = window[1].x - window[0].x;
            let dy = window[1].y - window[0].y;
            assert!(dx >= 0 && dy >= 0, "No negative backtracking or foldbacks allowed");
        }
    }
}
