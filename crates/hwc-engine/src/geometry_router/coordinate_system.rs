// ! Coordinate System Utilities
//!
//! Provides coordinate-system-aware utilities for working with cardinal directions,
//! bounding box edges, and routing directions that respect user-declared space orientation.
//!
//! v0.2.0: Eliminates hardcoded assumptions about X/Y axis orientation that broke
//! routing in non-default coordinate systems (e.g., `origin: tr by t`).

use crate::geometry::{BoundingBox, Point3D};

/// Cardinal direction in absolute space (independent of coordinate system).
///
/// These represent physical directions that are then mapped to coordinate system
/// axes based on the user-declared `OriginXY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalDirection {
    North,  // Physical "up" on the board
    South,  // Physical "down"
    East,   // Physical "right"
    West,   // Physical "left"
}

impl CardinalDirection {
    /// Get the direction vector in coordinate space given an origin.
    ///
    /// This properly accounts for the coordinate system orientation.
    ///
    /// # Examples
    /// ```
    /// use hwc_parser::OriginXY;
    ///
    /// // Bottom-Left origin (Y increases upward, X increases rightward)
    /// assert_eq!(CardinalDirection::North.direction_vector(OriginXY::BL), (0, 1));
    /// assert_eq!(CardinalDirection::East.direction_vector(OriginXY::BL), (1, 0));
    ///
    /// // Top-Left origin (Y increases downward, X increases rightward)
    /// assert_eq!(CardinalDirection::North.direction_vector(OriginXY::TL), (0, -1));
    /// assert_eq!(CardinalDirection::East.direction_vector(OriginXY::TL), (1, 0));
    ///
    /// // Top-Right origin (Y increases downward, X increases leftward)
    /// assert_eq!(CardinalDirection::North.direction_vector(OriginXY::TR), (0, -1));
    /// assert_eq!(CardinalDirection::East.direction_vector(OriginXY::TR), (-1, 0));
    /// ```
    pub fn direction_vector(&self, origin: hwc_parser::OriginXY) -> (i64, i64) {
        use hwc_parser::OriginXY;
        
        match (self, origin) {
            // Bottom-Left: Y↑ (North +Y), X→ (East +X)
            (Self::North, OriginXY::BL) => (0, 1),
            (Self::South, OriginXY::BL) => (0, -1),
            (Self::East, OriginXY::BL) => (1, 0),
            (Self::West, OriginXY::BL) => (-1, 0),
            
            // Bottom-Right: Y↑ (North +Y), X← (East -X)
            (Self::North, OriginXY::BR) => (0, 1),
            (Self::South, OriginXY::BR) => (0, -1),
            (Self::East, OriginXY::BR) => (-1, 0),
            (Self::West, OriginXY::BR) => (1, 0),
            
            // Top-Left: Y↓ (North -Y), X→ (East +X)
            (Self::North, OriginXY::TL) => (0, -1),
            (Self::South, OriginXY::TL) => (0, 1),
            (Self::East, OriginXY::TL) => (1, 0),
            (Self::West, OriginXY::TL) => (-1, 0),
            
            // Top-Right: Y↓ (North -Y), X← (East -X)
            (Self::North, OriginXY::TR) => (0, -1),
            (Self::South, OriginXY::TR) => (0, 1),
            (Self::East, OriginXY::TR) => (-1, 0),
            (Self::West, OriginXY::TR) => (1, 0),
        }
    }
    
    /// Infer cardinal direction from a vector (dx, dy) in coordinate space,
    /// given the coordinate system orientation.
    ///
    /// Returns the dominant cardinal direction based on the larger magnitude.
    pub fn from_vector(dx: i64, dy: i64, origin: hwc_parser::OriginXY) -> Self {
        use hwc_parser::OriginXY;
        
        if dx.abs() >= dy.abs() {
            // Horizontal dominant
            match origin {
                OriginXY::BL | OriginXY::TL => {
                    // X increases rightward
                    if dx > 0 { Self::East } else { Self::West }
                }
                OriginXY::BR | OriginXY::TR => {
                    // X increases leftward
                    if dx > 0 { Self::West } else { Self::East }
                }
            }
        } else {
            // Vertical dominant
            match origin {
                OriginXY::BL | OriginXY::BR => {
                    // Y increases upward
                    if dy > 0 { Self::North } else { Self::South }
                }
                OriginXY::TL | OriginXY::TR => {
                    // Y increases downward
                    if dy > 0 { Self::South } else { Self::North }
                }
            }
        }
    }
}

/// Get the bounding box edge point in a given cardinal direction.
///
/// This returns the appropriate min/max coordinate based on the direction
/// and coordinate system orientation.
///
/// # Arguments
/// * `bbox` - The bounding box
/// * `direction` - The cardinal direction to get the edge for
/// * `perpendicular_coord` - The coordinate along the perpendicular axis
/// * `z` - The Z coordinate
/// * `origin` - The coordinate system origin
///
/// # Examples
/// ```
/// // For a bbox with X:[100, 300], Y:[200, 400] in BL origin:
/// // East edge is at X=300 (bbox.max.x)
/// // West edge is at X=100 (bbox.min.x)
/// // North edge is at Y=400 (bbox.max.y)
/// // South edge is at Y=200 (bbox.min.y)
///
/// // For the same bbox in TR origin:
/// // East edge is at X=100 (bbox.min.x) - because X increases leftward!
/// // West edge is at X=300 (bbox.max.x)
/// // North edge is at Y=200 (bbox.min.y) - because Y increases downward!
/// // South edge is at Y=400 (bbox.max.y)
/// ```
pub fn get_bbox_edge_in_direction(
    bbox: &BoundingBox,
    direction: CardinalDirection,
    perpendicular_coord: i64,
    z: i64,
    origin: hwc_parser::OriginXY,
) -> Point3D {
    use hwc_parser::OriginXY;
    
    match (direction, origin) {
        // Bottom-Left: Y↑, X→
        (CardinalDirection::North, OriginXY::BL) => Point3D::new(perpendicular_coord, bbox.max.y, z),
        (CardinalDirection::South, OriginXY::BL) => Point3D::new(perpendicular_coord, bbox.min.y, z),
        (CardinalDirection::East, OriginXY::BL) => Point3D::new(bbox.max.x, perpendicular_coord, z),
        (CardinalDirection::West, OriginXY::BL) => Point3D::new(bbox.min.x, perpendicular_coord, z),
        
        // Bottom-Right: Y↑, X←
        (CardinalDirection::North, OriginXY::BR) => Point3D::new(perpendicular_coord, bbox.max.y, z),
        (CardinalDirection::South, OriginXY::BR) => Point3D::new(perpendicular_coord, bbox.min.y, z),
        (CardinalDirection::East, OriginXY::BR) => Point3D::new(bbox.min.x, perpendicular_coord, z),
        (CardinalDirection::West, OriginXY::BR) => Point3D::new(bbox.max.x, perpendicular_coord, z),
        
        // Top-Left: Y↓, X→
        (CardinalDirection::North, OriginXY::TL) => Point3D::new(perpendicular_coord, bbox.min.y, z),
        (CardinalDirection::South, OriginXY::TL) => Point3D::new(perpendicular_coord, bbox.max.y, z),
        (CardinalDirection::East, OriginXY::TL) => Point3D::new(bbox.max.x, perpendicular_coord, z),
        (CardinalDirection::West, OriginXY::TL) => Point3D::new(bbox.min.x, perpendicular_coord, z),
        
        // Top-Right: Y↓, X←
        (CardinalDirection::North, OriginXY::TR) => Point3D::new(perpendicular_coord, bbox.min.y, z),
        (CardinalDirection::South, OriginXY::TR) => Point3D::new(perpendicular_coord, bbox.max.y, z),
        (CardinalDirection::East, OriginXY::TR) => Point3D::new(bbox.min.x, perpendicular_coord, z),
        (CardinalDirection::West, OriginXY::TR) => Point3D::new(bbox.max.x, perpendicular_coord, z),
    }
}

/// Determine the routing direction between two points in any coordinate system.
///
/// This function is coordinate-system-agnostic and returns the cardinal direction
/// based on which axis has the larger delta.
///
/// # Arguments
/// * `from` - Starting point
/// * `to` - Ending point
///
/// # Returns
/// The cardinal direction of travel (based on dominant axis).
///
/// # Note
/// This function assumes the default BL (bottom-left) coordinate system.
/// For full coordinate-system awareness, you need access to the space's origin.
pub fn get_routing_direction(from: &Point3D, to: &Point3D) -> CardinalDirection {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    
    // Use BL (bottom-left) as default assumption
    // In proper usage, this should be passed the actual origin from HardwareSpace
    CardinalDirection::from_vector(dx, dy, hwc_parser::OriginXY::BL)
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
/// * `origin` - The coordinate system origin
///
/// # Returns
/// A waypoint at the via edge that, when stroked with flush ends, covers the via.
pub fn get_via_edge_waypoint(
    via_bbox: &BoundingBox,
    via_center: Point3D,
    routing_direction: CardinalDirection,
    origin: hwc_parser::OriginXY,
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
    
    get_bbox_edge_in_direction(via_bbox, opposite_direction, perp_coord, via_center.z, origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_parser::OriginXY;
    
    #[test]
    fn test_direction_vector_bl_origin() {
        // Bottom-Left: Y increases upward, X increases rightward
        assert_eq!(CardinalDirection::North.direction_vector(OriginXY::BL), (0, 1));
        assert_eq!(CardinalDirection::South.direction_vector(OriginXY::BL), (0, -1));
        assert_eq!(CardinalDirection::East.direction_vector(OriginXY::BL), (1, 0));
        assert_eq!(CardinalDirection::West.direction_vector(OriginXY::BL), (-1, 0));
    }
    
    #[test]
    fn test_direction_vector_tr_origin() {
        // Top-Right: Y increases downward, X increases leftward
        assert_eq!(CardinalDirection::North.direction_vector(OriginXY::TR), (0, -1));
        assert_eq!(CardinalDirection::South.direction_vector(OriginXY::TR), (0, 1));
        assert_eq!(CardinalDirection::East.direction_vector(OriginXY::TR), (-1, 0));
        assert_eq!(CardinalDirection::West.direction_vector(OriginXY::TR), (1, 0));
    }
    
    #[test]
    fn test_from_vector_bl_origin() {
        assert_eq!(CardinalDirection::from_vector(100, 0, OriginXY::BL), CardinalDirection::East);
        assert_eq!(CardinalDirection::from_vector(-100, 0, OriginXY::BL), CardinalDirection::West);
        assert_eq!(CardinalDirection::from_vector(0, 100, OriginXY::BL), CardinalDirection::North);
        assert_eq!(CardinalDirection::from_vector(0, -100, OriginXY::BL), CardinalDirection::South);
    }
    
    #[test]
    fn test_from_vector_tr_origin() {
        // In TR origin: +X is West, -X is East, +Y is South, -Y is North
        assert_eq!(CardinalDirection::from_vector(100, 0, OriginXY::TR), CardinalDirection::West);
        assert_eq!(CardinalDirection::from_vector(-100, 0, OriginXY::TR), CardinalDirection::East);
        assert_eq!(CardinalDirection::from_vector(0, 100, OriginXY::TR), CardinalDirection::South);
        assert_eq!(CardinalDirection::from_vector(0, -100, OriginXY::TR), CardinalDirection::North);
    }
    
    #[test]
    fn test_get_bbox_edge_bl_origin() {
        let bbox = BoundingBox::new(
            Point3D::new(100, 200, 0),
            Point3D::new(300, 400, 0),
        );
        
        // BL origin: Y↑, X→
        let north = get_bbox_edge_in_direction(&bbox, CardinalDirection::North, 200, 10, OriginXY::BL);
        assert_eq!(north, Point3D::new(200, 400, 10)); // max Y
        
        let east = get_bbox_edge_in_direction(&bbox, CardinalDirection::East, 300, 10, OriginXY::BL);
        assert_eq!(east, Point3D::new(300, 300, 10)); // max X
    }
    
    #[test]
    fn test_get_bbox_edge_tr_origin() {
        let bbox = BoundingBox::new(
            Point3D::new(100, 200, 0),
            Point3D::new(300, 400, 0),
        );
        
        // TR origin: Y↓, X←
        let north = get_bbox_edge_in_direction(&bbox, CardinalDirection::North, 200, 10, OriginXY::TR);
        assert_eq!(north, Point3D::new(200, 200, 10)); // min Y (because Y increases downward!)
        
        let east = get_bbox_edge_in_direction(&bbox, CardinalDirection::East, 300, 10, OriginXY::TR);
        assert_eq!(east, Point3D::new(100, 300, 10)); // min X (because X increases leftward!)
    }
}
