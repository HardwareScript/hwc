//! Heuristic functions for A* pathfinding

use crate::geometry::Point3D;

/// Calculate Manhattan distance heuristic.
///
/// This is the estimated cost from current to goal.
/// Manhattan distance is admissible (never overestimates) for Manhattan routing.
///
/// # Arguments
/// * `current` - Current position
/// * `goal` - Goal position
///
/// # Returns
/// Estimated cost to goal (Manhattan distance)
#[inline]
pub fn heuristic(current: Point3D, goal: Point3D) -> i64 {
    current.manhattan_distance(&goal)
}
