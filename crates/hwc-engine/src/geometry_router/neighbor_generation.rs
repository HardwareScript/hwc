//! Grid bounds for board-level spatial queries.
//!
//! Provides the `GridBounds` type used for bounds checking during
//! topological ray-casting routing.

use crate::geometry::Point3D;

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
