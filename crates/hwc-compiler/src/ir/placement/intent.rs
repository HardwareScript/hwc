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
