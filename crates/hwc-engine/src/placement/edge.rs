//! Edge enum for relative positioning (Sprint 3, Task 3.1)
//!
//! Defines the 6 faces of a 3D bounding box for anchor-based positioning.
//! This is used in the syntax: `at M1.right + 1mm`

/// Bounding box edge for relative positioning
///
/// Represents the 6 faces of a 3D bounding box:
/// - Left/Right: min/max X faces
/// - Top/Bottom: min/max Y faces  
/// - Front/Back: min/max Z faces
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    /// Min X face (left side)
    Left,
    /// Max X face (right side)
    Right,
    /// Max Y face (top side)
    Top,
    /// Min Y face (bottom side)
    Bottom,
    /// Min Z face (front side, lower layers)
    Front,
    /// Max Z face (back side, higher layers)
    Back,
    /// Explicit Min Z (alias for Front in default orientation)
    MinZ,
    /// Explicit Max Z (alias for Back in default orientation)
    MaxZ,
}

impl Edge {
    /// Get the opposite edge
    pub fn opposite(&self) -> Edge {
        match self {
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
            Edge::Top => Edge::Bottom,
            Edge::Bottom => Edge::Top,
            Edge::Front => Edge::Back,
            Edge::Back => Edge::Front,
            Edge::MinZ => Edge::MaxZ,
            Edge::MaxZ => Edge::MinZ,
        }
    }

    /// Check if this edge is on the X axis (Left/Right)
    pub fn is_x_axis(&self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }

    /// Check if this edge is on the Y axis (Top/Bottom)
    pub fn is_y_axis(&self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }

    /// Check if this edge is on the Z axis (Front/Back)
    pub fn is_z_axis(&self) -> bool {
        matches!(self, Edge::Front | Edge::Back | Edge::MinZ | Edge::MaxZ)
    }

    /// Get the primary axis direction for this edge
    /// Returns (dx, dy, dz) where one component is ±1 and others are 0
    pub fn direction_vector(&self) -> (i64, i64, i64) {
        match self {
            Edge::Left => (-1, 0, 0),
            Edge::Right => (1, 0, 0),
            Edge::Top => (0, 1, 0),
            Edge::Bottom => (0, -1, 0),
            Edge::Front => (0, 0, -1),
            Edge::Back => (0, 0, 1),
            Edge::MinZ => (0, 0, -1),
            Edge::MaxZ => (0, 0, 1),
        }
    }
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Edge::Left => write!(f, "left"),
            Edge::Right => write!(f, "right"),
            Edge::Top => write!(f, "top"),
            Edge::Bottom => write!(f, "bottom"),
            Edge::Front => write!(f, "front"),
            Edge::Back => write!(f, "back"),
            Edge::MinZ => write!(f, "min_z"),
            Edge::MaxZ => write!(f, "max_z"),
        }
    }
}
