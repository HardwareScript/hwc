/// Solution result from the QP solver.
#[derive(Clone, Debug)]
pub struct QpSolution {
    /// Optimized positions for each variable
    pub positions: Vec<(i64, i64)>,
    /// Whether the solver converged
    pub converged: bool,
    /// Number of iterations taken
    pub iterations: usize,
}

/// A simple iterative QP solver for trace legalization.
///
/// Minimizes displacement while maintaining clearance constraints.
/// For macro-scale problems (N >= 20), this provides a gradient-descent
/// approach. When clarabel is integrated, this will be replaced with
/// interior-point method for better convergence.
pub struct QpSolver {
    /// Maximum iterations before giving up
    pub max_iterations: usize,
    /// Convergence threshold (nm)
    pub convergence_threshold: i64,
    /// Step size for gradient descent
    pub step_size: f64,
}

impl QpSolver {
    pub fn new() -> Self {
        Self {
            max_iterations: 1000,
            convergence_threshold: 1,
            step_size: 0.5,
        }
    }

    /// Solve the legalization QP problem.
    ///
    /// Objective: minimize sum of squared displacements
    /// Subject to: minimum clearance constraints between all pairs
    ///
    /// `original_positions`: (x, y) for each variable
    /// `clearance_constraints`: pairs (i, j, min_distance) meaning variables i and j
    ///   must be at least min_distance apart
    /// `window_bounds`: (min_x, min_y, max_x, max_y) — variables must stay within
    #[inline]
    pub fn solve(
        &self,
        original_positions: &[(i64, i64)],
        clearance_constraints: &[(usize, usize, i64)],
        window_bounds: (i64, i64, i64, i64),
    ) -> QpSolution {
        let n = original_positions.len();
        if n == 0 {
            return QpSolution {
                positions: Vec::new(),
                converged: true,
                iterations: 0,
            };
        }

        let mut positions: Vec<(f64, f64)> = original_positions
            .iter()
            .map(|&(x, y)| (x as f64, y as f64))
            .collect();

        let (min_x, min_y, max_x, max_y) = window_bounds;
        let step = self.step_size;
        let threshold = self.convergence_threshold as f64;
        let mut converged = false;
        let mut iter = 0;

        for iteration in 0..self.max_iterations {
            iter = iteration + 1;
            let mut total_displacement_change = 0.0_f64;

            for &(i, j, min_dist) in clearance_constraints {
                if i >= n || j >= n {
                    continue;
                }
                let dx = positions[j].0 - positions[i].0;
                let dy = positions[j].1 - positions[i].1;
                let dist_sq = dx * dx + dy * dy;
                let min_dist_f = min_dist as f64;

                if dist_sq < min_dist_f * min_dist_f {
                    let dist = dist_sq.sqrt();
                    let overlap = if dist > 1e-9 {
                        min_dist_f - dist
                    } else {
                        // Degenerate: push along X as fallback
                        min_dist_f
                    };

                    let (nx, ny) = if dist > 1e-9 {
                        (dx / dist, dy / dist)
                    } else {
                        (1.0, 0.0)
                    };

                    let push_x = nx * overlap * step;
                    let push_y = ny * overlap * step;

                    let old_i = positions[i];
                    let old_j = positions[j];

                    positions[i].0 -= push_x;
                    positions[i].1 -= push_y;
                    positions[j].0 += push_x;
                    positions[j].1 += push_y;

                    total_displacement_change += (positions[i].0 - old_i.0).abs()
                        + (positions[i].1 - old_i.1).abs()
                        + (positions[j].0 - old_j.0).abs()
                        + (positions[j].1 - old_j.1).abs();
                }
            }

            // Clamp to window bounds
            for pos in &mut positions {
                pos.0 = pos.0.clamp(min_x as f64, max_x as f64);
                pos.1 = pos.1.clamp(min_y as f64, max_y as f64);
            }

            if total_displacement_change < threshold {
                converged = true;
                break;
            }
        }

        let result_positions: Vec<(i64, i64)> = positions
            .iter()
            .map(|&(x, y)| (x as i64, y as i64))
            .collect();

        QpSolution {
            positions: result_positions,
            converged,
            iterations: iter,
        }
    }
}

impl Default for QpSolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_constraints() {
        let solver = QpSolver::new();
        let positions = vec![(0, 0), (100, 100)];
        let result = solver.solve(&positions, &[], (-1000, -1000, 1000, 1000));
        assert!(result.converged);
        assert_eq!(result.positions, positions);
    }

    #[test]
    fn test_push_apart() {
        let solver = QpSolver::new();
        // Two points too close together (distance=10, min_dist=100)
        let positions = vec![(0, 0), (10, 0)];
        let constraints = vec![(0, 1, 100)];
        let result = solver.solve(&positions, &constraints, (-10000, -10000, 10000, 10000));
        assert!(result.converged);
        let dx = (result.positions[0].0 - result.positions[1].0).abs();
        assert!(dx >= 99, "Expected spacing >= 99, got {}", dx);
    }

    #[test]
    fn test_window_clamp() {
        let solver = QpSolver::new();
        let positions = vec![(0, 0)];
        let result = solver.solve(&positions, &[], (0, 0, 500, 500));
        assert!(result.converged);
        assert!(result.positions[0].0 >= 0);
        assert!(result.positions[0].0 <= 500);
    }
}
