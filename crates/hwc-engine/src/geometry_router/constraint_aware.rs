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
//! 1. Track voxels consumed so far (g-score)
//! 2. Calculate minimum remaining distance to goal (h-score)
//! 3. Project total path length: consumed + remaining
//! 4. Cost = |target_length - projected_length| * 10 + remaining
//!
//! If the router is moving too fast toward the goal, the cost increases,
//! forcing it to explore sideways (burning voxels) before approaching
//! the destination.

use crate::geometry::Point3D;
use crate::geometry_router::routing_patterns::RoutingPattern;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Node in the constraint-aware A* priority queue.
///
/// Unlike standard A*, this node tracks the target voxel count and
/// calculates cost based on how well the projected path matches that target.
#[derive(Clone, Eq, PartialEq)]
pub struct ConstraintNode {
    /// Current position in the grid
    pub position: Point3D,

    /// Voxels consumed so far (g-score)
    pub voxels_consumed: i64,

    /// Target total voxels we MUST hit
    pub target_voxels: i64,

    /// Cost (how far off we are from target length)
    pub cost: i64,

    /// Parent node for path reconstruction
    pub parent: Option<Box<ConstraintNode>>,

    /// Path history to prevent self-intersection during meanders
    pub path_history: FxHashSet<Point3D>,
}

impl Ord for ConstraintNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: lower cost is better, so we reverse the ordering
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.voxels_consumed.cmp(&other.voxels_consumed))
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
/// # Algorithm
///
/// 1. Calculate minimum remaining distance (Manhattan)
/// 2. Project total length: consumed + remaining
/// 3. Cost = |target - projected| * 10 + remaining
///
/// The *10 weight heavily penalizes target error, while the remaining
/// distance still provides a gradient toward the goal.
///
/// # Arguments
/// * `current_pos` - Current position
/// * `goal` - Goal position
/// * `voxels_consumed` - Voxels consumed so far
/// * `target_voxels` - Target total voxels
/// * `voxel_size_nm` - Voxel size in nanometers
///
/// # Returns
/// Cost representing how far off we are from target length
pub fn constraint_aware_heuristic(
    current_pos: Point3D,
    goal: Point3D,
    voxels_consumed: i64,
    target_voxels: i64,
    voxel_size_nm: i64,
) -> i64 {
    let manhattan_nm = (current_pos.x - goal.x).abs()
        + (current_pos.y - goal.y).abs()
        + (current_pos.z - goal.z).abs();

    let minimum_remaining_voxels = manhattan_nm / voxel_size_nm;
    let projected_total = voxels_consumed + minimum_remaining_voxels;

    // The core insight: Cost is how far off we are from the exact target length.
    // We add minimum_remaining_voxels to the cost to still pull the router towards the goal
    // when multiple paths have the same target error.
    let target_error = (target_voxels - projected_total).abs();

    // Weight the target error heavily, but still preserve a gradient towards the goal
    (target_error * 10) + minimum_remaining_voxels
}

/// Route a net with constraint-aware A* pathfinding.
///
/// **Architecture Reference:** CONSTRAINT-AWARE-ROUTING.md Phase 1-2
///
/// This is the core implementation that feeds length constraints into
/// the pathfinding algorithm BEFORE routing starts.
///
/// # Arguments
/// * `start` - Starting position
/// * `goal` - Goal position
/// * `target_voxels` - Exact number of voxels the path must consume
/// * `pattern` - Optional routing pattern for macro-moves
/// * `occupied_voxels` - Set of currently occupied voxels
/// * `bounds` - Grid bounds (max_x, max_y, max_z)
/// * `voxel_size_nm` - Voxel size in nanometers
///
/// # Returns
/// Path from start to goal with exact target length, or error message
pub fn constraint_aware_astar(
    start: Point3D,
    goal: Point3D,
    target_voxels: i64,
    pattern: &Option<RoutingPattern>,
    occupied_voxels: &FxHashSet<Point3D>,
    bounds: (i64, i64, i64),
    voxel_size_nm: i64,
) -> Result<Vec<Point3D>, String> {
    let mut frontier = BinaryHeap::new();

    // Instead of tracking min voxels_consumed, we track the best (lowest) heuristic cost
    // to reach a specific coordinate. This prevents the router from defaulting to shortest-path.
    let mut best_cost_to_reach = FxHashMap::default();

    let mut start_history = FxHashSet::default();
    start_history.insert(start);

    let start_node = ConstraintNode {
        position: start,
        voxels_consumed: 0,
        target_voxels,
        cost: constraint_aware_heuristic(start, goal, 0, target_voxels, voxel_size_nm),
        parent: None,
        path_history: start_history,
    };

    frontier.push(start_node);
    best_cost_to_reach.insert(start, 0);

    // Limit iterations to prevent infinite loops on impossible constraints
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
            // Reconstruct path
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

        // 1. Generate standard single-voxel moves
        let standard_neighbors =
            generate_standard_neighbors(current.position, voxel_size_nm, bounds);
        for neighbor in standard_neighbors {
            next_moves.push(vec![neighbor]);
        }

        // 2. Generate Pattern Macro-Moves if available and we need to burn voxels
        let remaining_manhattan = ((current.position.x - goal.x).abs()
            + (current.position.y - goal.y).abs())
            / voxel_size_nm;

        if let Some(pat) = pattern {
            // Only inject macro-moves if we are under budget
            if current.voxels_consumed + remaining_manhattan < target_voxels {
                for heading in [0, 90, 180, 270] {
                    let macro_move = pat.generate_moves(current.position, heading, voxel_size_nm);
                    if !macro_move.is_empty() {
                        next_moves.push(macro_move);
                    }
                }
            }
        }

        // Evaluate all generated moves
        for move_sequence in next_moves {
            let mut valid = true;
            let mut step_history = current.path_history.clone();

            // Check collision and self-intersection for the entire macro-move sequence
            for step_pos in &move_sequence {
                if occupied_voxels.contains(step_pos) || step_history.contains(step_pos) {
                    valid = false;
                    break;
                }
                step_history.insert(*step_pos);
            }

            if !valid {
                continue;
            }

            let final_pos = *move_sequence.last().unwrap();
            let new_voxels_consumed = current.voxels_consumed + move_sequence.len() as i64;

            let new_cost = constraint_aware_heuristic(
                final_pos,
                goal,
                new_voxels_consumed,
                target_voxels,
                voxel_size_nm,
            );

            // Accept the move if it improves our heuristic error for reaching this coordinate,
            // or if we haven't reached this coordinate yet.
            if new_cost < *best_cost_to_reach.get(&final_pos).unwrap_or(&i64::MAX) {
                best_cost_to_reach.insert(final_pos, new_cost);

                // Build the parent chain for the macro-move
                let mut parent_node = Some(Box::new(current.clone()));

                // If it's a macro-move (length > 1), we need to insert the intermediate steps
                // into the path chain so they are reconstructed correctly.
                for (i, &intermediate_pos) in move_sequence
                    .iter()
                    .enumerate()
                    .take(move_sequence.len() - 1)
                {
                    let intermediate_node = ConstraintNode {
                        position: intermediate_pos,
                        voxels_consumed: current.voxels_consumed + (i as i64) + 1,
                        target_voxels,
                        cost: new_cost,
                        parent: parent_node,
                        path_history: FxHashSet::default(), // Not needed for intermediate reconstruction
                    };
                    parent_node = Some(Box::new(intermediate_node));
                }

                frontier.push(ConstraintNode {
                    position: final_pos,
                    voxels_consumed: new_voxels_consumed,
                    target_voxels,
                    cost: new_cost,
                    parent: parent_node,
                    path_history: step_history,
                });
            }
        }
    }

    Err("No valid path found matching the constraints.".into())
}

/// Generate standard single-voxel neighbors.
fn generate_standard_neighbors(
    pos: Point3D,
    voxel_size: i64,
    bounds: (i64, i64, i64),
) -> Vec<Point3D> {
    let mut neighbors = Vec::new();
    let (max_x, max_y, max_z) = bounds;

    if pos.x + voxel_size < max_x {
        neighbors.push(Point3D::new(pos.x + voxel_size, pos.y, pos.z));
    }
    if pos.x - voxel_size >= 0 {
        neighbors.push(Point3D::new(pos.x - voxel_size, pos.y, pos.z));
    }
    if pos.y + voxel_size < max_y {
        neighbors.push(Point3D::new(pos.x, pos.y + voxel_size, pos.z));
    }
    if pos.y - voxel_size >= 0 {
        neighbors.push(Point3D::new(pos.x, pos.y - voxel_size, pos.z));
    }
    if pos.z + voxel_size < max_z {
        neighbors.push(Point3D::new(pos.x, pos.y, pos.z + voxel_size));
    }
    if pos.z - voxel_size >= 0 {
        neighbors.push(Point3D::new(pos.x, pos.y, pos.z - voxel_size));
    }

    neighbors
}
