//! Pathfinding state management

use crate::geometry::Point3D;
use crate::geometry_router::deterministic_pathfinder::{
    DeterministicCost, DeterministicPriorityQueue,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Pathfinding state for A* algorithm.
///
/// Uses [`DeterministicPriorityQueue`] for priority queue with deterministic
/// tie-breaking that guarantees the same input always produces the same pop order.
#[derive(Debug)]
pub struct PathfindingState {
    /// Priority queue for f-score ordering with deterministic tie-breaking
    pub(super) priority_queue: DeterministicPriorityQueue,

    /// Visited set for cycle detection
    pub(super) visited: FxHashSet<Point3D>,

    /// Parent tracking for path reconstruction
    pub(super) came_from: FxHashMap<Point3D, Point3D>,

    /// Cost from start to this node (g-score)
    pub(super) cost_so_far: FxHashMap<Point3D, i64>,
}

impl PathfindingState {
    /// Create a new pathfinding state.
    pub(crate) fn new() -> Self {
        Self {
            priority_queue: DeterministicPriorityQueue::new(),
            visited: FxHashSet::default(),
            came_from: FxHashMap::default(),
            cost_so_far: FxHashMap::default(),
        }
    }

    /// Add a node to the frontier with deterministic tie-breaking.
    ///
    /// Uses `g_score` and position coordinates to break ties so that
    /// equal-cost nodes are always expanded in the same order.
    pub(super) fn add_node(&mut self, position: Point3D, f_score: i64, g_score: i64) {
        let h = f_score - g_score;
        let cost = DeterministicCost::from_point(position, f_score, g_score, h, 0);
        self.priority_queue.push(position, cost);
    }

    /// Get the next node from the frontier.
    pub(super) fn pop_node(&mut self) -> Option<Point3D> {
        self.priority_queue.pop().map(|(pos, _)| pos)
    }

    /// Check if frontier is empty.
    pub(super) fn is_empty(&self) -> bool {
        self.priority_queue.is_empty()
    }
}

/// Reconstruct path from came_from map.
pub(super) fn reconstruct_path(
    came_from: &FxHashMap<Point3D, Point3D>,
    start: Point3D,
    goal: Point3D,
) -> Vec<Point3D> {
    let mut path = vec![goal];
    let mut current = goal;

    while current != start {
        if let Some(&parent) = came_from.get(&current) {
            path.push(parent);
            current = parent;
        } else {
            break;
        }
    }

    path.reverse();
    path
}
