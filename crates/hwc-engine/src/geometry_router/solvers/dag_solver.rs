/// A constraint in the 1D DAG: `from_position + min_gap <= to_position`
#[derive(Clone, Debug)]
pub struct DagConstraint {
    pub from_idx: usize,
    pub to_idx: usize,
    pub min_gap: i64,
}

/// A 1D DAG graph compaction solver.
///
/// Solves longest-path constraints for 1D trace compaction.
/// Given a set of traces on a line with minimum gap constraints,
/// finds the minimum-area placement that satisfies all constraints.
///
/// This is used for micro-adjustments (N < 20) where the overhead
/// of a full QP solver is not justified.
pub struct DagSolver;

impl DagSolver {
    /// Solve 1D compaction using longest-path on the constraint DAG.
    ///
    /// `positions`: initial positions of elements (1D)
    /// `constraints`: gap constraints between elements
    /// Returns the optimized positions.
    #[inline]
    pub fn solve_1d(positions: &[i64], constraints: &[DagConstraint]) -> Vec<i64> {
        let n = positions.len();
        if n == 0 {
            return Vec::new();
        }

        // Build adjacency list
        let mut adj: Vec<Vec<(usize, i64)>> = vec![Vec::new(); n];
        for c in constraints {
            if c.from_idx < n && c.to_idx < n {
                adj[c.from_idx].push((c.to_idx, c.min_gap));
            }
        }

        // Topological sort (Kahn's algorithm)
        let mut in_degree = vec![0i64; n];
        for c in constraints {
            if c.to_idx < n {
                in_degree[c.to_idx] += 1;
            }
        }

        let mut queue: std::collections::VecDeque<usize> =
            (0..n).filter(|&i| in_degree[i] == 0).collect();

        let mut order = Vec::with_capacity(n);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &(next, _) in &adj[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        // Longest path (forward pass)
        let mut result = positions.to_vec();
        for &node in &order {
            for &(next, gap) in &adj[node] {
                let earliest = result[node] + gap;
                if earliest > result[next] {
                    result[next] = earliest;
                }
            }
        }

        result
    }

    /// Solve 2D compaction by running 1D solver on X and Y axes independently.
    #[inline]
    pub fn solve_2d(
        positions: &[(i64, i64)],
        x_constraints: &[DagConstraint],
        y_constraints: &[DagConstraint],
    ) -> Vec<(i64, i64)> {
        let x_orig: Vec<i64> = positions.iter().map(|p| p.0).collect();
        let y_orig: Vec<i64> = positions.iter().map(|p| p.1).collect();
        let x_opt = Self::solve_1d(&x_orig, x_constraints);
        let y_opt = Self::solve_1d(&y_orig, y_constraints);
        x_opt.into_iter().zip(y_opt).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_constraints() {
        let positions = vec![10, 20, 30];
        let result = DagSolver::solve_1d(&positions, &[]);
        assert_eq!(result, positions);
    }

    #[test]
    fn test_simple_chain() {
        // A -> B (gap 5) -> C (gap 5)
        let positions = vec![0, 0, 0];
        let constraints = vec![
            DagConstraint {
                from_idx: 0,
                to_idx: 1,
                min_gap: 5,
            },
            DagConstraint {
                from_idx: 1,
                to_idx: 2,
                min_gap: 5,
            },
        ];
        let result = DagSolver::solve_1d(&positions, &constraints);
        assert_eq!(result, vec![0, 5, 10]);
    }

    #[test]
    fn test_already_satisfied() {
        let positions = vec![0, 100, 200];
        let constraints = vec![
            DagConstraint {
                from_idx: 0,
                to_idx: 1,
                min_gap: 5,
            },
            DagConstraint {
                from_idx: 1,
                to_idx: 2,
                min_gap: 5,
            },
        ];
        let result = DagSolver::solve_1d(&positions, &constraints);
        assert_eq!(result, positions);
    }

    #[test]
    fn test_2d_compaction() {
        let positions = vec![(0, 0), (50, 50)];
        let x_constraints = vec![DagConstraint {
            from_idx: 0,
            to_idx: 1,
            min_gap: 100,
        }];
        let y_constraints = vec![DagConstraint {
            from_idx: 0,
            to_idx: 1,
            min_gap: 100,
        }];
        let result = DagSolver::solve_2d(&positions, &x_constraints, &y_constraints);
        assert_eq!(result, vec![(0, 0), (100, 100)]);
    }
}
