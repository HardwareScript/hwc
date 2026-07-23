//! Placement Intent - Explicit semantic precision for component placement
//!
//! This eliminates the "lossy conversion" bug where `at Region.center` loses
//! the information that we're placing at CENTER (not corner).
//!
//! Architecture Alignment:
//! - Assembly tier (absolute coords) -> PlacementIntent::Corner
//! - Middle tier (Region.center) -> PlacementIntent::Center
//! - High tier (declarative) -> PlacementIntent::Center (auto-placed by solver)

use hwc_engine::geometry::Point3D;

/// Explicit placement semantics - what does this point represent?
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlacementIntent {
    /// Place component corner (bottom-left) at this point
    /// Used for: absolute coordinates, relative `.left`/`.bottom`/`.top_left` anchors
    Corner(Point3D),

    /// Place component CENTER at this point
    /// Used for: relative `.center` anchors
    Center(Point3D),
}

impl PlacementIntent {
    /// Extract raw point (for mounting/elevation calculations)
    pub fn point(&self) -> Point3D {
        match self {
            PlacementIntent::Corner(p) | PlacementIntent::Center(p) => *p,
        }
    }

    /// Calculate actual component origin (corner) given dimensions
    /// This is THE SINGLE SOURCE OF TRUTH for centering math
    pub fn calculate_origin(&self, width_nm: i64, height_nm: i64, depth_nm: i64) -> Point3D {
        match self {
            PlacementIntent::Corner(p) => *p,

            PlacementIntent::Center(p) => {
                // Center-to-corner conversion: offset backwards by half dimensions
                Point3D::new(
                    p.x - (width_nm / 2),
                    p.y - (height_nm / 2),
                    p.z - (depth_nm / 2),
                )
            }
        }
    }

    /// Check if this requires component dimensions for proper placement
    pub fn requires_dimensions(&self) -> bool {
        matches!(self, PlacementIntent::Center(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_passthrough() {
        let intent = PlacementIntent::Corner(Point3D::new(1000, 2000, 3000));
        let origin = intent.calculate_origin(500, 600, 700);
        assert_eq!(origin, Point3D::new(1000, 2000, 3000));
    }

    #[test]
    fn center_to_origin() {
        let intent = PlacementIntent::Center(Point3D::new(5000, 5000, 0));
        let origin = intent.calculate_origin(2000, 1000, 500);
        assert_eq!(origin, Point3D::new(4000, 4500, -250));
    }

    #[test]
    fn center_odd_dimensions() {
        // Odd dimensions: 1001 / 2 = 500 (integer division)
        let intent = PlacementIntent::Center(Point3D::new(5000, 5000, 0));
        let origin = intent.calculate_origin(1001, 1001, 1001);
        assert_eq!(origin, Point3D::new(4499, 4499, -500));
    }

    #[test]
    fn requires_dimensions() {
        assert!(!PlacementIntent::Corner(Point3D::new(0, 0, 0)).requires_dimensions());
        assert!(PlacementIntent::Center(Point3D::new(0, 0, 0)).requires_dimensions());
    }
}
