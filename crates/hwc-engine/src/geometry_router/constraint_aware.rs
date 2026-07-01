//! Constraint-Aware A* Pathfinding
//!
//! This module implements constraint-aware routing where length matching
//! constraints are fed into the A* algorithm BEFORE routing starts, not
//! as post-processing.
//!
//! **Architecture Reference:** CONSTRAINT-AWARE-ROUTING.md
//!
//! # Core Insight
//!
//! Standard A* minimizes path length. For length matching, we need paths
//! of a SPECIFIC length. The solution: modify the A* heuristic to penalize
//! paths that don't match the target length.
//!
//! # Algorithm
//!
//! 1. Track physical length consumed so far (g-score in nm)
//! 2. Calculate minimum remaining distance to goal (h-score in nm)
//! 3. Project total path length: consumed + remaining
//! 4. Cost = |target_length - projected_length| * 10 + remaining
//!
//! If the router is moving too fast toward the goal, the cost increases,
//! forcing it to explore sideways (burning length) before approaching
//! the destination.

use crate::geometry::Point3D;
use crate::geometry_router::routing_patterns::RoutingPattern;
use rustc_hash::FxHashSet;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Node in the constraint-aware A* priority queue.
///
/// Unlike standard A*, this node tracks the target length and
/// calculates cost based on how well the projected path matches that target.
#[derive(Clone, Eq, PartialEq)]
pub struct ConstraintNode {
    /// Current position in physical coordinates
    pub position: Point3D,

    /// Physical nanometers consumed so far (g-score)
    pub length_nm: i64,

    /// Target total nanometers we MUST hit
    pub target_length_nm: i64,

    /// Cost (how far off we are from target length)
    pub cost: i64,

    /// Parent node for path reconstruction
    pub parent: Option<Box<ConstraintNode>>,

    /// Path history to prevent self-intersection during meanders
    pub path_history: FxHashSet<Point3D>,
}

impl Ord for ConstraintNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.length_nm.cmp(&other.length_nm))
    }
}

impl PartialOrd for ConstraintNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Calculate constraint-aware heuristic.
///
/// **Key Innovation:** Instead of minimizing distance, we minimize the
/// difference between projected total length and target length.
///
/// # Arguments
/// * `current_pos` - Current position
/// * `goal` - Goal position
/// * `length_nm` - Physical nanometers consumed so far
/// * `target_length_nm` - Target total nanometers
///
/// # Returns
/// Cost representing how far off we are from target length
pub fn constraint_aware_heuristic(
    current_pos: Point3D,
    goal: Point3D,
    length_nm: i64,
    target_length_nm: i64,
) -> i64 {
    let manhattan_nm = (current_pos.x - goal.x).abs()
        + (current_pos.y - goal.y).abs()
        + (current_pos.z - goal.z).abs();

    let minimum_remaining_nm = manhattan_nm;
    let projected_total = length_nm + minimum_remaining_nm;

    let target_error = (target_length_nm - projected_total).abs();

    (target_error * 10) + minimum_remaining_nm
}

/// Route a net with constraint-aware A* pathfinding.
///
/// **Architecture Reference:** CONSTRAINT-AWARE-ROUTING.md Phase 1-2
///
/// # Arguments
/// * `start` - Starting position
/// * `goal` - Goal position
/// * `target_length_nm` - Exact physical nanometers the path must consume
/// * `pattern` - Optional routing pattern for macro-moves
/// * `bounds` - Grid bounds (max_x, max_y, max_z)
/// * `resolution_nm` - Snap resolution in nanometers
///
/// # Returns
/// Path from start to goal with exact target length, or error message
pub fn constraint_aware_astar(
    start: Point3D,
    goal: Point3D,
    target_length_nm: i64,
    pattern: &Option<RoutingPattern>,
    bounds: (i64, i64, i64),
    resolution_nm: i64,
) -> Result<Vec<Point3D>, String> {
    let mut frontier = BinaryHeap::new();

    let mut best_cost_to_reach = rustc_hash::FxHashMap::default();

    let mut start_history = FxHashSet::default();
    start_history.insert(start);

    let start_node = ConstraintNode {
        position: start,
        length_nm: 0,
        target_length_nm,
        cost: constraint_aware_heuristic(start, goal, 0, target_length_nm),
        parent: None,
        path_history: start_history,
    };

    frontier.push(start_node);
    best_cost_to_reach.insert(start, 0);

    let mut iterations = 0;
    let max_iterations = 50_000;

    while let Some(current) = frontier.pop() {
        iterations += 1;
        if iterations > max_iterations {
            return Err(format!(
                "Constraint-aware routing timed out after {} iterations",
                max_iterations
            ));
        }

        if current.position == goal {
            let mut path = Vec::new();
            let mut curr = Some(Box::new(current));
            while let Some(node) = curr {
                path.push(node.position);
                curr = node.parent;
            }
            path.reverse();
            return Ok(path);
        }

        let mut next_moves = Vec::new();

        let standard_neighbors =
            generate_standard_neighbors(current.position, resolution_nm, bounds);
        for neighbor in standard_neighbors {
            next_moves.push(vec![neighbor]);
        }

        let remaining_manhattan = (current.position.x - goal.x).abs()
            + (current.position.y - goal.y).abs();

        if let Some(pat) = pattern {
            if current.length_nm + remaining_manhattan < target_length_nm {
                for heading in [0, 90, 180, 270] {
                    let macro_move = pat.generate_moves(current.position, heading, resolution_nm);
                    if !macro_move.is_empty() {
                        next_moves.push(macro_move);
                    }
                }
            }
        }

        for move_sequence in next_moves {
            let mut valid = true;
            let mut step_history = current.path_history.clone();

            for step_pos in &move_sequence {
                if step_history.contains(step_pos) {
                    valid = false;
                    break;
                }
                step_history.insert(*step_pos);
            }

            if !valid {
                continue;
            }

            let final_pos = *move_sequence.last().unwrap();
            let new_length_nm = current.length_nm + move_sequence.len() as i64 * resolution_nm;

            let new_cost = constraint_aware_heuristic(
                final_pos,
                goal,
                new_length_nm,
                target_length_nm,
            );

            if new_cost < *best_cost_to_reach.get(&final_pos).unwrap_or(&i64::MAX) {
                best_cost_to_reach.insert(final_pos, new_cost);

                let mut parent_node = Some(Box::new(current.clone()));

                for (i, &intermediate_pos) in move_sequence
                    .iter()
                    .enumerate()
                    .take(move_sequence.len() - 1)
                {
                    let intermediate_node = ConstraintNode {
                        position: intermediate_pos,
                        length_nm: current.length_nm + ((i as i64) + 1) * resolution_nm,
                        target_length_nm,
                        cost: new_cost,
                        parent: parent_node,
                        path_history: FxHashSet::default(),
                    };
                    parent_node = Some(Box::new(intermediate_node));
                }

                frontier.push(ConstraintNode {
                    position: final_pos,
                    length_nm: new_length_nm,
                    target_length_nm,
                    cost: new_cost,
                    parent: parent_node,
                    path_history: step_history,
                });
            }
        }
    }

    Err("No valid path found matching the constraints.".into())
}

/// Generate standard single-resolution neighbors.
fn generate_standard_neighbors(
    pos: Point3D,
    resolution_nm: i64,
    bounds: (i64, i64, i64),
) -> Vec<Point3D> {
    let mut neighbors = Vec::new();
    let (max_x, max_y, max_z) = bounds;

    if pos.x + resolution_nm < max_x {
        neighbors.push(Point3D::new(pos.x + resolution_nm, pos.y, pos.z));
    }
    if pos.x - resolution_nm >= 0 {
        neighbors.push(Point3D::new(pos.x - resolution_nm, pos.y, pos.z));
    }
    if pos.y + resolution_nm < max_y {
        neighbors.push(Point3D::new(pos.x, pos.y + resolution_nm, pos.z));
    }
    if pos.y - resolution_nm >= 0 {
        neighbors.push(Point3D::new(pos.x, pos.y - resolution_nm, pos.z));
    }
    if pos.z + resolution_nm < max_z {
        neighbors.push(Point3D::new(pos.x, pos.y, pos.z + resolution_nm));
    }
    if pos.z - resolution_nm >= 0 {
        neighbors.push(Point3D::new(pos.x, pos.y, pos.z - resolution_nm));
    }

    neighbors
}
