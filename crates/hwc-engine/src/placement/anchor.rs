//! Physical Anchor System (Task B3)
//!
//! Anchors allow designers to specify hard constraints on component placement.
//! This bridges the gap between "fully automatic" and "fully manual" placement.
//!
//! **The Problem**: Floorplanner places gates automatically, but professional designers
//! need to say: "This high-speed connector MUST be on the right edge of the board"
//! or "This CPU must be in the center."
//!
//! **The Solution**: Geometric Anchors
//! - `Edge(Right)` - Component must be on the right edge
//! - `Point(x, y)` - Component must be at exact position
//! - `Region(x1, y1, x2, y2)` - Component must be within bounded area
//!
//! **Performance**: O(1) anchor constraint checking per component

use crate::geometry::Point3D;

/// Anchor constraint for component placement
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Anchor {
    /// No anchor - component can be placed anywhere (default)
    #[default]
    None,

    /// Edge anchor - component must be on specified board edge
    /// Examples: Edge(Right), Edge(Left), Edge(Top), Edge(Bottom)
    Edge(EdgePosition),

    /// Point anchor - component must be at exact position
    /// Position is in nanometers
    Point(Point3D),

    /// Region anchor - component must be within bounded area
    /// Bounds are in nanometers: (min_x, min_y, max_x, max_y)
    /// Z coordinate is not constrained by region anchors
    Region {
        min_x: i64,
        min_y: i64,
        max_x: i64,
        max_y: i64,
    },
}

/// Edge position for edge anchors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgePosition {
    Left,
    Right,
    Top,
    Bottom,
}

impl Anchor {
    /// Check if this anchor is compatible with a proposed position
    ///
    /// # Arguments
    /// * `position` - Proposed position in nanometers
    /// * `board_size` - Board dimensions in nanometers (width, height, depth)
    /// * `component_size` - Component dimensions in nanometers (width, height, depth)
    ///
    /// # Returns
    /// `true` if the position satisfies the anchor constraint
    pub fn is_compatible(
        &self,
        position: &Point3D,
        board_size: (i64, i64, i64),
        component_size: (i64, i64, i64),
    ) -> bool {
        match self {
            Anchor::None => true, // No constraint

            Anchor::Edge(edge) => {
                let (board_width, board_height, _board_depth) = board_size;
                let (comp_width, comp_height, _comp_depth) = component_size;

                // Define edge threshold (10% of board dimension)
                let x_threshold = board_width / 10;
                let y_threshold = board_height / 10;

                match edge {
                    EdgePosition::Left => position.x < x_threshold,
                    EdgePosition::Right => position.x + comp_width > board_width - x_threshold,
                    EdgePosition::Top => position.y + comp_height > board_height - y_threshold,
                    EdgePosition::Bottom => position.y < y_threshold,
                }
            }

            Anchor::Point(target) => {
                // Allow small tolerance (1% of component size)
                let tolerance_x = component_size.0 / 100;
                let tolerance_y = component_size.1 / 100;
                let tolerance_z = component_size.2 / 100;

                (position.x - target.x).abs() <= tolerance_x
                    && (position.y - target.y).abs() <= tolerance_y
                    && (position.z - target.z).abs() <= tolerance_z
            }

            Anchor::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let (comp_width, comp_height, _comp_depth) = component_size;

                // Component must be fully within region
                position.x >= *min_x
                    && position.y >= *min_y
                    && position.x + comp_width <= *max_x
                    && position.y + comp_height <= *max_y
            }
        }
    }

    /// Get the priority of this anchor (higher = more constrained)
    ///
    /// Used to resolve conflicts when multiple components have anchors.
    /// Point anchors have highest priority, followed by Edge, then Region.
    pub fn priority(&self) -> u8 {
        match self {
            Anchor::None => 0,
            Anchor::Region { .. } => 1,
            Anchor::Edge(_) => 2,
            Anchor::Point(_) => 3,
        }
    }

    /// Calculate the ideal position for this anchor
    ///
    /// # Arguments
    /// * `board_size` - Board dimensions in nanometers (width, height, depth)
    /// * `component_size` - Component dimensions in nanometers (width, height, depth)
    ///
    /// # Returns
    /// Ideal position in nanometers, or None if anchor doesn't specify a position
    pub fn ideal_position(
        &self,
        board_size: (i64, i64, i64),
        component_size: (i64, i64, i64),
    ) -> Option<Point3D> {
        match self {
            Anchor::None => None,

            Anchor::Edge(edge) => {
                let (board_width, board_height, board_depth) = board_size;
                let (comp_width, comp_height, _comp_depth) = component_size;

                // Place component at edge with small margin
                let margin = 1_000_000; // 1mm margin from edge

                let (x, y) = match edge {
                    EdgePosition::Left => (margin, board_height / 2 - comp_height / 2),
                    EdgePosition::Right => (
                        board_width - comp_width - margin,
                        board_height / 2 - comp_height / 2,
                    ),
                    EdgePosition::Top => (
                        board_width / 2 - comp_width / 2,
                        board_height - comp_height - margin,
                    ),
                    EdgePosition::Bottom => (board_width / 2 - comp_width / 2, margin),
                };

                Some(Point3D {
                    x,
                    y,
                    z: board_depth / 2, // Middle layer by default
                })
            }

            Anchor::Point(target) => Some(*target),

            Anchor::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                // Place at center of region
                let center_x = (min_x + max_x) / 2;
                let center_y = (min_y + max_y) / 2;

                Some(Point3D {
                    x: center_x - component_size.0 / 2,
                    y: center_y - component_size.1 / 2,
                    z: board_size.2 / 2, // Middle layer by default
                })
            }
        }
    }

    /// Check if two anchors conflict (both try to place at incompatible positions)
    ///
    /// # Returns
    /// `true` if the anchors are incompatible
    pub fn conflicts_with(&self, other: &Anchor) -> bool {
        match (self, other) {
            // Point anchors conflict if they're at different positions
            (Anchor::Point(p1), Anchor::Point(p2)) => p1 != p2,

            // Edge anchors conflict if they're on opposite edges
            (Anchor::Edge(e1), Anchor::Edge(e2)) => {
                matches!(
                    (e1, e2),
                    (EdgePosition::Left, EdgePosition::Right)
                        | (EdgePosition::Right, EdgePosition::Left)
                        | (EdgePosition::Top, EdgePosition::Bottom)
                        | (EdgePosition::Bottom, EdgePosition::Top)
                )
            }

            // Other combinations don't necessarily conflict
            _ => false,
        }
    }
}

impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Anchor::None => write!(f, "None"),
            Anchor::Edge(edge) => write!(f, "Edge({:?})", edge),
            Anchor::Point(p) => write!(f, "Point({}, {}, {})", p.x, p.y, p.z),
            Anchor::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => write!(f, "Region({}, {}, {}, {})", min_x, min_y, max_x, max_y),
        }
    }
}
