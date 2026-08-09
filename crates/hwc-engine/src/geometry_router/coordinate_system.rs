// ! Coordinate System Utilities
//!
//! Provides utilities for working with cardinal directions, bounding box edges,
//! and routing directions.
//!
//! v0.2.1 (Bloat Purge Category 1.1): The user-facing `origin:` declaration is
//! purged. Every space uses the single canonical coordinate system:
//! **Bottom-Left / Z-Up** (X increases rightward, Y increases upward,
//! Z increases from the bottom of the stackup upward). All functions here are
//! hardcoded to that convention — there are no origin parameters and no
//! axis-inversion branches.

use crate::geometry::{BoundingBox, Point3D};

/// Cardinal direction in absolute space.
///
/// Because the coordinate system is canonically Bottom-Left, these physical
/// directions map directly onto the coordinate axes with no inversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalDirection {
    North, // Physical "up" on the board  (+Y)
    South, // Physical "down"             (-Y)
    East,  // Physical "right"            (+X)
    West,  // Physical "left"             (-X)
}

impl CardinalDirection {
    /// Unit vector for this direction in canonical Bottom-Left space.
    pub fn direction_vector(&self) -> (i64, i64) {
        // Canonical Bottom-Left: Y increases upward, X increases rightward.
        match self {
            Self::North => (0, 1),
            Self::South => (0, -1),
            Self::East => (1, 0),
            Self::West => (-1, 0),
        }
    }

    /// Infer cardinal direction from a vector (dx, dy) in canonical
    /// Bottom-Left coordinate space.
    ///
    /// Returns the dominant cardinal direction based on the larger magnitude.
    pub fn from_vector(dx: i64, dy: i64) -> Self {
        if dx.abs() >= dy.abs() {
            // Horizontal dominant: X increases rightward.
            if dx > 0 {
                Self::East
            } else {
                Self::West
            }
        } else {
            // Vertical dominant: Y increases upward.
            if dy > 0 {
                Self::North
            } else {
                Self::South
            }
        }
    }
}

/// Get the bounding box edge point in a given cardinal direction.
///
/// # Arguments
/// * `bbox` - The bounding box
/// * `direction` - The cardinal direction to get the edge for
/// * `perpendicular_coord` - The coordinate along the perpendicular axis
/// * `z` - The Z coordinate
///
/// # Examples
/// For a bbox with X:[100, 300], Y:[200, 400] in canonical Bottom-Left space:
/// - East edge is at X=300 (bbox.max.x)
/// - West edge is at X=100 (bbox.min.x)
/// - North edge is at Y=400 (bbox.max.y)
/// - South edge is at Y=200 (bbox.min.y)
pub fn get_bbox_edge_in_direction(
    bbox: &BoundingBox,
    direction: CardinalDirection,
    perpendicular_coord: i64,
    z: i64,
) -> Point3D {
    // Canonical Bottom-Left: Y increases upward, X increases rightward.
    match direction {
        CardinalDirection::North => Point3D::new(perpendicular_coord, bbox.max.y, z),
        CardinalDirection::South => Point3D::new(perpendicular_coord, bbox.min.y, z),
        CardinalDirection::East => Point3D::new(bbox.max.x, perpendicular_coord, z),
        CardinalDirection::West => Point3D::new(bbox.min.x, perpendicular_coord, z),
    }
}

/// Determine the routing direction between two points.
///
/// # Arguments
/// * `from` - Starting point
/// * `to` - Ending point
///
/// # Returns
/// The cardinal direction of travel (based on dominant axis).
pub fn get_routing_direction(from: &Point3D, to: &Point3D) -> CardinalDirection {
    let dx = to.x - from.x;
    let dy = to.y - from.y;

    CardinalDirection::from_vector(dx, dy)
}

/// Get the via edge waypoint that ensures proper trace coverage when routing
/// from or to a via/contact pad.
///
/// This accounts for the fact that traces are stroked with flush ends (EndType::Butt),
/// so the waypoint must be positioned at the via edge in the routing direction to
/// ensure the stroked geometry covers the full via pad.
///
/// # Arguments
/// * `via_bbox` - The via/contact bounding box
/// * `via_center` - The via center point (for perpendicular coordinate)
/// * `routing_direction` - The direction routing is traveling
///
/// # Returns
/// A waypoint at the via edge that, when stroked with flush ends, covers the via.
pub fn get_via_edge_waypoint(
    via_bbox: &BoundingBox,
    via_center: Point3D,
    routing_direction: CardinalDirection,
) -> Point3D {
    // For routing IN a direction, we want the waypoint at the edge OPPOSITE that direction
    // Example: routing East from a via → waypoint at West edge
    // When stroked eastward with flush end, the geometry covers the via
    let opposite_direction = match routing_direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::West => CardinalDirection::East,
    };

    // Get perpendicular coordinate from via center
    let perp_coord = match routing_direction {
        CardinalDirection::North | CardinalDirection::South => via_center.x,
        CardinalDirection::East | CardinalDirection::West => via_center.y,
    };

    get_bbox_edge_in_direction(via_bbox, opposite_direction, perp_coord, via_center.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_vector() {
        // Canonical Bottom-Left: Y increases upward, X increases rightward
        assert_eq!(CardinalDirection::North.direction_vector(), (0, 1));
        assert_eq!(CardinalDirection::South.direction_vector(), (0, -1));
        assert_eq!(CardinalDirection::East.direction_vector(), (1, 0));
        assert_eq!(CardinalDirection::West.direction_vector(), (-1, 0));
    }

    #[test]
    fn test_from_vector() {
        assert_eq!(
            CardinalDirection::from_vector(100, 0),
            CardinalDirection::East
        );
        assert_eq!(
            CardinalDirection::from_vector(-100, 0),
            CardinalDirection::West
        );
        assert_eq!(
            CardinalDirection::from_vector(0, 100),
            CardinalDirection::North
        );
        assert_eq!(
            CardinalDirection::from_vector(0, -100),
            CardinalDirection::South
        );
    }

    #[test]
    fn test_get_bbox_edge() {
        let bbox = BoundingBox::new(Point3D::new(100, 200, 0), Point3D::new(300, 400, 0));

        // Canonical Bottom-Left: Y↑, X→
        let north = get_bbox_edge_in_direction(&bbox, CardinalDirection::North, 200, 10);
        assert_eq!(north, Point3D::new(200, 400, 10)); // max Y

        let east = get_bbox_edge_in_direction(&bbox, CardinalDirection::East, 300, 10);
        assert_eq!(east, Point3D::new(300, 300, 10)); // max X
    }
}
