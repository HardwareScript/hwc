//! Deterministic Neighbor Generation for Pathfinding
//!
//! This module provides stable, deterministic neighbor generation
//! for A* pathfinding to ensure reproducible builds.

use super::layer_direction::is_valid_move;
use crate::constraint_manager::LayerDirection;
use crate::geometry::{Direction, Point3D};

/// Grid bounds for neighbor generation.
///
/// Simple struct to hold grid dimensions for bounds checking.
#[derive(Debug, Clone, Copy)]
pub struct GridBounds {
    pub width_nm: i64,
    pub height_nm: i64,
    pub depth_nm: i64,
}

impl GridBounds {
    /// Create new grid bounds.
    pub const fn new(width_nm: i64, height_nm: i64, depth_nm: i64) -> Self {
        Self {
            width_nm,
            height_nm,
            depth_nm,
        }
    }

    /// Check if a point is within bounds.
    #[inline]
    pub fn contains(&self, point: Point3D) -> bool {
        point.z >= 0
            && point.z <= self.depth_nm
            && point.x >= 0
            && point.x <= self.width_nm
            && point.y >= 0
            && point.y <= self.height_nm
    }
}

/// Get neighbors in stable order for deterministic pathfinding.
///
/// Returns neighbors in FIXED order: North, South, East, West, Up, Down.
/// This ensures that the same input always produces the same output,
/// which is critical for reproducible builds.
///
/// **Algorithm**:
/// 1. Generate neighbors in fixed order (North, South, East, West, Up, Down)
/// 2. Filter by layer direction rules (Manhattan routing)
/// 3. Filter by grid bounds
/// 4. Return stable-ordered vector
///
/// # Arguments
/// * `cell` - Current cell position
/// * `bounds` - Grid bounds for bounds checking
/// * `layer_direction` - Direction restriction for this layer
/// * `resolution_nm` - Step size in nanometers
///
/// # Returns
/// Vector of valid neighbor positions in stable order
pub fn get_neighbors_stable(
    cell: Point3D,
    bounds: GridBounds,
    layer_direction: LayerDirection,
    resolution_nm: i64,
) -> Vec<Point3D> {
    // Fixed order for determinism: North, South, East, West, Up, Down
    let directions = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];

    let mut neighbors = Vec::with_capacity(6);

    for dir in &directions {
        let neighbor = cell.move_direction(*dir, resolution_nm);

        // Check bounds
        if !bounds.contains(neighbor) {
            continue;
        }

        // Check layer direction rules
        if !is_valid_move(cell, neighbor, layer_direction) {
            continue;
        }

        neighbors.push(neighbor);
    }

    neighbors
}
