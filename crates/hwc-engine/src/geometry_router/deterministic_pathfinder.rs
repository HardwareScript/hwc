//! Deterministic tie-breaking A* pathfinder heuristics.
//!
//! When multiple nodes have equal f-cost, this module enforces a strict
//! total ordering so that the same input always produces the same path.
//! This is critical for reproducible PCB builds.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::geometry::Point3D;

/// Cost tuple with deterministic tie-breaking.
///
/// Ordering priority:
/// 1. Lower f-cost
/// 2. Lower g-cost (prefer nodes closer to start)
/// 3. Direction priority (horizontal over vertical by default)
/// 4. Lower z-coordinate, then lower x-coordinate, then lower y-coordinate
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicCost {
    pub f: i64,
    pub g: i64,
    pub h: i64,
    /// 0 = horizontal move, 1 = vertical move (lower wins tie)
    pub direction_penalty: u8,
    pub z: i64,
    pub x: i64,
    pub y: i64,
}

impl DeterministicCost {
    #[inline]
    pub fn new(f: i64, g: i64, h: i64, direction_penalty: u8, z: i64, x: i64, y: i64) -> Self {
        Self { f, g, h, direction_penalty, z, x, y }
    }

    /// Create a cost from a Point3D position and score components.
    #[inline]
    pub fn from_point(pos: Point3D, f: i64, g: i64, h: i64, direction_penalty: u8) -> Self {
        Self { f, g, h, direction_penalty, z: pos.z, x: pos.x, y: pos.y }
    }
}

/// Reverse ordering for BinaryHeap (min-heap via max-heap inversion).
///
/// The `BinaryHeap` is a max-heap, so we reverse the comparison to get
/// a min-heap that pops the lowest cost first.
impl Ord for DeterministicCost {
    fn cmp(&self, other: &Self) -> Ordering {
        // Fully reversed comparison for min-heap via BinaryHeap (max-heap).
        // Lower values on every field → "greater" Ord → popped first.
        other.f.cmp(&self.f)
            .then_with(|| other.g.cmp(&self.g))
            .then_with(|| other.direction_penalty.cmp(&self.direction_penalty))
            .then_with(|| other.z.cmp(&self.z))
            .then_with(|| other.x.cmp(&self.x))
            .then_with(|| other.y.cmp(&self.y))
    }
}

impl PartialOrd for DeterministicCost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Direction preference for tie-breaking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionPriority {
    /// Prefer horizontal moves over vertical when costs match.
    HorizontalFirst,
    /// Prefer vertical moves over horizontal when costs match.
    VerticalFirst,
    /// Prefer the direction toward the goal.
    LowestCoordinate,
}

impl Default for DirectionPriority {
    fn default() -> Self {
        Self::HorizontalFirst
    }
}

/// A priority queue entry pairing a position with its deterministic cost.
///
/// `Ord` is derived entirely from `DeterministicCost`, so the position
/// stored alongside is carried through without affecting sort order.
#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub cost: DeterministicCost,
    pub position: Point3D,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}
impl Eq for QueueEntry {}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cost.cmp(&other.cost)
    }
}
impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A priority queue with deterministic ordering for A* pathfinding.
///
/// Wraps `BinaryHeap` with a custom `Ord` implementation that guarantees
/// the same input always produces the same pop order.
#[derive(Debug)]
pub struct DeterministicPriorityQueue {
    heap: BinaryHeap<QueueEntry>,
}

impl DeterministicPriorityQueue {
    pub fn new() -> Self {
        Self { heap: BinaryHeap::new() }
    }

    /// Push a node with its position and cost into the queue.
    #[inline]
    pub fn push(&mut self, pos: Point3D, cost: DeterministicCost) {
        self.heap.push(QueueEntry { cost, position: pos });
    }

    /// Pop the node with the lowest cost (deterministic tie-breaking).
    #[inline]
    pub fn pop(&mut self) -> Option<(Point3D, DeterministicCost)> {
        self.heap.pop().map(|entry| (entry.position, entry.cost))
    }

    /// Returns true if the queue is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Number of elements in the queue.
    #[inline]
    pub fn len(&self) -> usize {
        self.heap.len()
    }
}

impl Default for DeterministicPriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute direction penalty for a move from `from` to `to`.
///
/// Returns 0 for horizontal moves, 1 for vertical moves.
/// Diagonal moves (both dx and dy nonzero) return 0 (horizontal priority).
#[inline]
pub fn direction_penalty(from: (i64, i64), to: (i64, i64)) -> u8 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    if dy != 0 && dx == 0 { 1 } else { 0 }
}

/// Generate neighbors with deterministic ordering.
///
/// Returns neighbors sorted so that horizontal moves come before vertical
/// moves when costs match. This ensures the A* expansion order is
/// deterministic regardless of the order neighbors are generated.
pub fn astar_step_tiebreak(
    current: (i64, i64),
    _goal: (i64, i64),
    g_cost: i64,
    direction: DirectionPriority,
) -> Vec<((i64, i64), i64)> {
    let mut neighbors = Vec::with_capacity(4);

    // Generate all 4 cardinal neighbors
    let deltas: &[(i64, i64)] = &[(1, 0), (-1, 0), (0, 1), (0, -1)];

    for &(dx, dy) in deltas {
        let nx = current.0 + dx;
        let ny = current.1 + dy;
        let step_g = g_cost + 1;
        neighbors.push(((nx, ny), step_g));
    }

    // Sort deterministically based on direction priority
    match direction {
        DirectionPriority::HorizontalFirst => {
            neighbors.sort_by(|a, b| {
                let a_horiz = (a.0).0 != current.0;
                let b_horiz = (b.0).0 != current.0;
                b_horiz.cmp(&a_horiz)
                    .then_with(|| (a.0).0.cmp(&(b.0).0))
                    .then_with(|| (a.0).1.cmp(&(b.0).1))
            });
        }
        DirectionPriority::VerticalFirst => {
            neighbors.sort_by(|a, b| {
                let a_vert = (a.0).1 != current.1;
                let b_vert = (b.0).1 != current.1;
                b_vert.cmp(&a_vert)
                    .then_with(|| (a.0).0.cmp(&(b.0).0))
                    .then_with(|| (a.0).1.cmp(&(b.0).1))
            });
        }
        DirectionPriority::LowestCoordinate => {
            neighbors.sort_by(|a, b| {
                (a.0).0.cmp(&(b.0).0).then_with(|| (a.0).1.cmp(&(b.0).1))
            });
        }
    }

    neighbors
}

/// Verify that two paths are identical (determinism check).
///
/// Returns `true` if both paths have the same length and every
/// corresponding point is equal.
#[inline]
pub fn verify_deterministic_path(path1: &[(i64, i64)], path2: &[(i64, i64)]) -> bool {
    path1.len() == path2.len() && path1.iter().zip(path2.iter()).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_ordering_same_f_different_g() {
        let c1 = DeterministicCost::new(10, 3, 7, 0, 0, 0, 0);
        let c2 = DeterministicCost::new(10, 5, 5, 0, 0, 0, 0);
        // Lower g wins (c1.g=3 < c2.g=5), so c1 should be "less" (popped first)
        assert!(c1 > c2); // reversed for min-heap: higher Ord = popped first
    }

    #[test]
    fn cost_ordering_same_fg_horizontal_preferred() {
        let c1 = DeterministicCost::new(10, 5, 5, 0, 0, 1, 0); // horizontal
        let c2 = DeterministicCost::new(10, 5, 5, 1, 0, 1, 0); // vertical
        // Lower direction_penalty wins (0 < 1)
        assert!(c1 > c2);
    }

    #[test]
    fn cost_ordering_same_everything_lower_coord_preferred() {
        let c1 = DeterministicCost::new(10, 5, 5, 0, 0, 3, 7);
        let c2 = DeterministicCost::new(10, 5, 5, 0, 0, 3, 9);
        // Lower y wins (7 < 9)
        assert!(c1 > c2);
    }

    #[test]
    fn cost_ordering_same_everything_lower_x_preferred() {
        let c1 = DeterministicCost::new(10, 5, 5, 0, 0, 2, 0);
        let c2 = DeterministicCost::new(10, 5, 5, 0, 0, 5, 0);
        // Lower x wins (2 < 5)
        assert!(c1 > c2);
    }

    #[test]
    fn cost_ordering_different_f() {
        let c1 = DeterministicCost::new(5, 3, 2, 0, 0, 0, 0);
        let c2 = DeterministicCost::new(10, 5, 5, 0, 0, 0, 0);
        // Lower f wins
        assert!(c1 > c2);
    }

    #[test]
    fn priority_queue_deterministic_pop() {
        let mut pq = DeterministicPriorityQueue::new();
        pq.push(Point3D::new(1, 0, 0), DeterministicCost::new(10, 5, 5, 0, 0, 1, 0));
        pq.push(Point3D::new(2, 0, 0), DeterministicCost::new(10, 5, 5, 1, 0, 1, 0));
        pq.push(Point3D::new(3, 0, 0), DeterministicCost::new(10, 3, 7, 0, 0, 1, 0));

        let first = pq.pop();
        assert!(first.is_some());
        let (pos, _) = first.unwrap();
        // g=3 wins over g=5
        assert_eq!(pos, Point3D::new(3, 0, 0));
    }

    #[test]
    fn priority_queue_same_cost_deterministic() {
        let mut pq = DeterministicPriorityQueue::new();
        // Push many nodes with same f and g but different positions
        for x in 0..10 {
            for y in 0..10 {
                pq.push(
                    Point3D::new(x, y, 0),
                    DeterministicCost::new(10, 5, 5, 0, 0, x, y),
                );
            }
        }

        // Pop all and verify they come out in deterministic order
        let mut popped = Vec::new();
        while let Some((pos, _)) = pq.pop() {
            popped.push(pos);
        }

        // Should be sorted by (x, y) since z is all 0
        let mut sorted = popped.clone();
        sorted.sort_by_key(|p| (p.x, p.y));
        assert_eq!(popped, sorted);
    }

    #[test]
    fn astar_step_deterministic_ordering() {
        let current = (5, 5);
        let goal = (10, 5);
        let neighbors = astar_step_tiebreak(current, goal, 0, DirectionPriority::HorizontalFirst);

        assert_eq!(neighbors.len(), 4);

        // Horizontal moves should come first
        let first_is_horizontal = neighbors[0].0 .1 == current.1;
        assert!(first_is_horizontal);
    }

    #[test]
    fn astar_step_vertical_first() {
        let current = (5, 5);
        let goal = (5, 10);
        let neighbors = astar_step_tiebreak(current, goal, 0, DirectionPriority::VerticalFirst);

        assert_eq!(neighbors.len(), 4);

        // Vertical moves should come first
        let first_is_vertical = neighbors[0].0 .0 == current.0;
        assert!(first_is_vertical);
    }

    #[test]
    fn verify_deterministic_path_same() {
        let path1 = vec![(0, 0), (1, 0), (2, 0), (3, 0)];
        let path2 = vec![(0, 0), (1, 0), (2, 0), (3, 0)];
        assert!(verify_deterministic_path(&path1, &path2));
    }

    #[test]
    fn verify_deterministic_path_different() {
        let path1 = vec![(0, 0), (1, 0), (2, 0)];
        let path2 = vec![(0, 0), (0, 1), (0, 2)];
        assert!(!verify_deterministic_path(&path1, &path2));
    }

    #[test]
    fn verify_deterministic_path_different_length() {
        let path1 = vec![(0, 0), (1, 0)];
        let path2 = vec![(0, 0), (1, 0), (2, 0)];
        assert!(!verify_deterministic_path(&path1, &path2));
    }

    #[test]
    fn direction_penalty_horizontal() {
        assert_eq!(direction_penalty((0, 0), (1, 0)), 0);
        assert_eq!(direction_penalty((0, 0), (-1, 0)), 0);
    }

    #[test]
    fn direction_penalty_vertical() {
        assert_eq!(direction_penalty((0, 0), (0, 1)), 1);
        assert_eq!(direction_penalty((0, 0), (0, -1)), 1);
    }

    #[test]
    fn direction_penalty_diagonal_treated_as_horizontal() {
        // Diagonal is not generated by astar_step_tiebreak, but if it were,
        // it would be treated as horizontal (dx != 0)
        assert_eq!(direction_penalty((0, 0), (1, 1)), 0);
    }

    #[test]
    fn priority_queue_empty() {
        let mut pq = DeterministicPriorityQueue::new();
        assert!(pq.is_empty());
        assert_eq!(pq.len(), 0);
        assert!(pq.pop().is_none());
    }

    #[test]
    fn priority_queue_push_pop_roundtrip() {
        let mut pq = DeterministicPriorityQueue::new();
        pq.push(
            Point3D::new(10, 20, 0),
            DeterministicCost::new(5, 2, 3, 0, 0, 10, 20),
        );
        assert_eq!(pq.len(), 1);
        let (pos, cost) = pq.pop().unwrap();
        assert_eq!(pos, Point3D::new(10, 20, 0));
        assert_eq!(cost.f, 5);
        assert!(pq.is_empty());
    }
}
