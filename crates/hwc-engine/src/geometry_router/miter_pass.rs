//! v0.1.8: 45-Degree Mitered Chamfer Pass
//!
//! Converts sharp 90° corners in routed traces into smooth 45° diagonal
//! chamfers. This maintains constant trace impedance (Z₀) across bends,
//! preventing signal reflection and EMI radiation on high-speed lines.
//!
//! **Physics:** A sharp 90° corner widens the effective trace width by √2,
//! creating a parasitic capacitive load that drops impedance. The miter pass
//! replaces the corner with a diagonal segment that preserves constant width.

use crate::geometry::Point3D;

/// Post-routing miter/chamfer engine.
///
/// Scans waypoint lists for 90° corners and replaces them with 45° diagonal
/// chamfers. The miter distance is calculated from the trace width to maintain
/// constant impedance across the bend.
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

    /// Apply 45° miter chamfers to all 90° corners in a path.
    ///
    /// For each consecutive triple (P₀ → P₁ → P₂) that forms a 90° corner:
    /// 1. Calculate rollback distance `d = trace_width × 1.5`
    /// 2. Insert Pₐ = P₁ - d·û₁ and Pᵦ = P₁ + d·û₂
    /// 3. Replace the sharp corner with Pₐ → Pᵦ (45° diagonal)
    ///
    /// # Arguments
    /// * `path` - Ordered list of waypoints (minimum 3 points)
    ///
    /// # Returns
    /// New waypoint list with 90° corners replaced by 45° chamfers
    pub fn apply_miter_pass(&self, path: &[Point3D]) -> Vec<Point3D> {
        if path.len() < 3 {
            return path.to_vec();
        }

        let mut mitered = Vec::with_capacity(path.len() + path.len() / 2);
        mitered.push(path[0]);

        // Miter distance: 1.5× trace width for standard impedance stability
        let miter_dist = self.trace_width_nm * 3 / 2;

        let mut i = 1;
        while i < path.len() - 1 {
            let p_prev = path[i - 1];
            let p_curr = path[i];
            let p_next = path[i + 1];

            // Direction vectors
            let v1_x = p_curr.x - p_prev.x;
            let v1_y = p_curr.y - p_prev.y;
            let v2_x = p_next.x - p_curr.x;
            let v2_y = p_next.y - p_curr.y;

            // Check for 90° corner: orthogonal segments in the XY plane
            // (Z is ignored — miter is a 2D operation on the routing layer)
            let dot = v1_x * v2_x + v1_y * v2_y;
            let is_corner = dot == 0
                && (v1_x != 0 || v1_y != 0)
                && (v2_x != 0 || v2_y != 0);

            if is_corner {
                let len1_f = ((v1_x * v1_x + v1_y * v1_y) as f64).sqrt();
                let len2_f = ((v2_x * v2_x + v2_y * v2_y) as f64).sqrt();
                let len1 = len1_f as i64;
                let len2 = len2_f as i64;

                if len1 > miter_dist && len2 > miter_dist {
                    // v0.1.9.1 BUG FIX: Use floating-point for unit vector calculation
                    // Previous code used integer division (v1_x / len1) which caused
                    // massive coordinate corruption when results rounded to 0 or 1.
                    // Unit direction vectors (floating point)
                    let u1_x_f = v1_x as f64 / len1_f;
                    let u1_y_f = v1_y as f64 / len1_f;
                    let u2_x_f = v2_x as f64 / len2_f;
                    let u2_y_f = v2_y as f64 / len2_f;

                    // Miter start: rollback from corner along incoming direction
                    let p_a = Point3D::new(
                        p_curr.x - (u1_x_f * miter_dist as f64).round() as i64,
                        p_curr.y - (u1_y_f * miter_dist as f64).round() as i64,
                        p_curr.z,
                    );
                    // Miter end: advance from corner along outgoing direction
                    let p_b = Point3D::new(
                        p_curr.x + (u2_x_f * miter_dist as f64).round() as i64,
                        p_curr.y + (u2_y_f * miter_dist as f64).round() as i64,
                        p_curr.z,
                    );

                    mitered.push(p_a);
                    mitered.push(p_b);
                } else {
                    // Segment too short for miter, keep original corner
                    mitered.push(p_curr);
                }
            } else {
                // Not a 90° corner, keep as-is
                mitered.push(p_curr);
            }
            i += 1;
        }

        mitered.push(*path.last().unwrap());
        mitered
    }

    /// Apply miter pass to all paths in a route result.
    ///
    /// Mutates the paths in-place, replacing 90° corners with 45° chamfers.
    pub fn apply_to_paths(&self, paths: &mut Vec<Vec<Point3D>>) {
        for path in paths.iter_mut() {
            if path.len() >= 3 {
                *path = self.apply_miter_pass(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_miter_on_short_path() {
        let engine = MiterEngine::new(200_000);
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(1_000_000, 0, 0),
        ];
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
        assert_eq!(result.len(), 3);
        assert_eq!(result, path);
    }

    #[test]
    fn test_miter_90_degree_corner() {
        let engine = MiterEngine::new(200_000);
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(5_000_000, 0, 0),      // corner
            Point3D::new(5_000_000, 5_000_000, 0),
        ];
        let result = engine.apply_miter_pass(&path);

        // Should have: start, miter_a, miter_b, end = 4 points
        assert_eq!(result.len(), 4);
        // Miter points should be on the diagonal
        assert!(result[1].x < 5_000_000); // p_a rolled back
        assert!(result[2].y > 0);          // p_b advanced
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
}
