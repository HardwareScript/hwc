use crate::geometry::BoundingBox;
use compact_str::CompactString;

/// A tracked obstacle with its metadata and pre-computed XY inflation.
#[derive(Debug, Clone)]
pub struct TrackedObstacle {
    /// The original (uninflated) bounding box of the obstacle in nanometers.
    pub original_bbox: BoundingBox,

    /// The Minkowski-inflated bounding box used for collision queries.
    /// Expands original_bbox by `inflation_nm` in X and Y directions.
    pub inflated_bbox: BoundingBox,

    /// The inflation margin applied in nanometers.
    /// Computed as: trace_width_nm / 2 + clearance_nm
    pub inflation_nm: i64,

    /// The Z-layer / plane this obstacle lives on (derived from bbox min.z).
    /// Used for layer-specific queries.
    pub layer_z_nm: i64,

    /// Name of the obstacle (component name, net name, etc.)
    pub name: CompactString,

    /// Type descriptor (e.g., "Component", "Trace", "Via", "Keepout")
    pub obstacle_type: CompactString,
}
