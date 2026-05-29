//! # BoundingBoxTracker: Minkowski Obstacle Inflation (v0.1.7)
//!
//! **Architectural Reference:**
//! - `Docs/v0.1.7/ROUTING-AND-MANUFACTURING-ARCHITECTURE.md` (Section 4.2)
//! - `ROADMAP/v0.1.7/BASE-IMPLEMENTATION-ROADMAP.md` (Section 1.2)
//!
//! ## Purpose
//! Instead of allocating billions of voxels to enforce clearances, the router
//! utilizes **Minkowski Sum / Obstacle Inflation**:
//!
//! 1. Queries the `BoundingBoxTracker` for all obstacle AABBs on the active layer.
//! 2. Inflates each obstacle's bounding box by: $Inflation = \frac{Width}{2} + Clearance$.
//! 3. The pathfinder routes an infinitesimally thin mathematical ray around these
//!    inflated boundaries, guaranteeing exact clearance with O(1) collision overhead.
//!
//! ## The Math
//! Given a trace of width W and a minimum clearance C:
//! - The Minkowski sum of the trace (a segment of width W) with an obstacle AABB
//!   is equivalent to expanding the AABB by (W/2 + C) in XY directions while
//!   keeping full Z extent.
//! - Since we route a "zero-width" ray, the inflated AABB automatically enforces
//!   both trace width clearance AND inter-net clearance in one operation.
//!
//! ## Integration
//! - **Pass 2 (Obstacle Blitting)**: Components and previously routed traces are
//!   registered as obstacles with their inflation parameters.
//! - **Pass 3 (Parallel 2.5D Auto-Routing)**: The SDF generator uses inflated
//!   AABBs for analytic distance calculation - obstacles are already "fat".
//! - **No voxel-level clearance checking needed** - clearance is baked into the
//!   obstacle geometry itself.

use crate::geometry::{BoundingBox, Point3D};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

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

/// BoundingBoxTracker: The spatial index for Minkowski-inflated obstacle queries.
///
/// This replaces the need for per-voxel clearance checking by storing obstacles
/// with pre-computed inflation margins. The pathfinder's "zero-width" ray must
/// simply avoid intersecting any inflated AABB, and all clearance constraints
/// are automatically satisfied.
///
/// **Performance**: O(N) per query where N = number of obstacles on that layer.
/// Since typical designs have 10-1000 obstacles per layer, this is effectively
/// constant-time compared to scanning billions of voxels.
///
/// # Example
/// ```
/// use hwc_engine::geometry_router::BoundingBoxTracker;
/// use hwc_engine::geometry::{BoundingBox, Point3D};
///
/// let mut tracker = BoundingBoxTracker::new();
///
/// // Register a component obstacle with trace width and clearance
/// let component_bbox = BoundingBox::new(
///     Point3D::new(10_000_000, 10_000_000, 500_000),
///     Point3D::new(12_000_000, 14_000_000, 500_000),
/// );
///
/// // Trace width = 0.2mm (200_000nm), clearance = 0.15mm (150_000nm)
/// // Inflation = 200_000/2 + 150_000 = 250_000nm
/// tracker.register_obstacle(
///     component_bbox,
///     200_000,  // trace_width_nm
///     150_000,  // clearance_nm
///     "U1".into(),
///     "Component".into(),
/// );
///
/// // Query obstacles on the same layer
/// let obstacles = tracker.query_layer(500_000);
/// assert_eq!(obstacles.len(), 1);
///
/// // The inflated bbox should be expanded by exactly 250_000nm in XY
/// let inflated = &obstacles[0].inflated_bbox;
/// assert_eq!(inflated.min.x, 10_000_000 - 250_000);  // 9_750_000
/// assert_eq!(inflated.max.x, 12_000_000 + 250_000);  // 12_250_000
/// ```
pub struct BoundingBoxTracker {
    /// All registered obstacles indexed by Z-plane.
    /// Key: Z-height in nanometers (layer_z_nm)
    /// Value: Vec of obstacles on that layer
    by_layer: FxHashMap<i64, Vec<TrackedObstacle>>,

    /// All obstacles in a flat list (for global collision checks)
    all_obstacles: Vec<TrackedObstacle>,

    /// Number of registered obstacles
    count: usize,
}

impl BoundingBoxTracker {
    /// Create a new empty BoundingBoxTracker.
    pub fn new() -> Self {
        Self {
            by_layer: FxHashMap::default(),
            all_obstacles: Vec::new(),
            count: 0,
        }
    }

    /// Register an obstacle with Minkowski inflation.
    ///
    /// # Arguments
    /// * `bbox` - The original (uninflated) bounding box of the obstacle in nanometers.
    /// * `trace_width_nm` - Width of the trace being routed (nanometers).
    /// * `clearance_nm` - Minimum clearance to other nets (nanometers).
    /// * `name` - Name of the obstacle for debugging/error reporting.
    /// * `obstacle_type` - Type descriptor (e.g., "Component", "Trace").
    ///
    /// # Returns
    /// The inflation margin that was applied.
    pub fn register_obstacle(
        &mut self,
        bbox: BoundingBox,
        trace_width_nm: i64,
        clearance_nm: i64,
        name: CompactString,
        obstacle_type: CompactString,
    ) -> i64 {
        // Minkowski inflation formula:
        // inflation = trace_width / 2 + clearance
        // This ensures the inflated AABB guarantees both:
        // 1. Trace width clearance (trace won't overlap with obstacle)
        // 2. Inter-net clearance (trace won't violate minimum spacing)
        let half_width = trace_width_nm / 2;
        let inflation_nm = half_width + clearance_nm;

        // Apply inflation only in XY (Z remains un-inflated for planar routing)
        // The Minkowski sum of a segment with width W and an AABB:
        // - In XY: Expand by half-width in all directions
        // - In Z: Obstacles block the full Z height of their layer
        let inflated_bbox = BoundingBox {
            min: Point3D::new(
                bbox.min.x - inflation_nm,
                bbox.min.y - inflation_nm,
                bbox.min.z,
            ),
            max: Point3D::new(
                bbox.max.x + inflation_nm,
                bbox.max.y + inflation_nm,
                bbox.max.z,
            ),
        };

        // Determine the Z-plane for this obstacle (use mid-Z of the bbox)
        let layer_z_nm = bbox.min.z;

        let obstacle = TrackedObstacle {
            original_bbox: bbox,
            inflated_bbox,
            inflation_nm,
            layer_z_nm,
            name,
            obstacle_type,
        };

        // Insert into layer index
        self.by_layer
            .entry(layer_z_nm)
            .or_default()
            .push(obstacle.clone());

        // Maintain flat list
        self.all_obstacles.push(obstacle);
        self.count += 1;

        inflation_nm
    }

    /// Register a component obstacle from the GeometryRouter's add_component_obstacle.
    ///
    /// This is a convenience method that extracts trace width and clearance
    /// from the context.
    ///
    /// # Arguments
    /// * `bbox` - The component's bounding box in nanometers.
    /// * `trace_width_nm` - Trace width for inflation calculation.
    /// * `clearance_nm` - Clearance for inflation calculation.
    /// * `name` - Component instance name.
    /// * `component_type` - Component type description.
    pub fn register_component(
        &mut self,
        bbox: BoundingBox,
        trace_width_nm: i64,
        clearance_nm: i64,
        name: CompactString,
        component_type: CompactString,
    ) -> i64 {
        self.register_obstacle(bbox, trace_width_nm, clearance_nm, name, component_type)
    }

    /// Register a previously routed trace as an obstacle.
    ///
    /// Traces on the same layer block future routing. The trace's own width
    /// contributes to the inflation for the NEXT trace routed alongside it.
    ///
    /// # Arguments
    /// * `start` - Start point of the trace segment (nanometers).
    /// * `end` - End point of the trace segment (nanometers).
    /// * `existing_trace_width_nm` - Width of the existing trace.
    /// * `new_trace_width_nm` - Width of the trace being routed now.
    /// * `clearance_nm` - Minimum inter-trace clearance.
    /// * `net_name` - Name of the net this trace belongs to.
    pub fn register_trace(
        &mut self,
        start: Point3D,
        end: Point3D,
        existing_trace_width_nm: i64,
        new_trace_width_nm: i64,
        clearance_nm: i64,
        net_name: CompactString,
    ) -> i64 {
        // Build bounding box from trace segment (including its own half-width)
        let half_existing = existing_trace_width_nm / 2;
        let bbox = BoundingBox {
            min: Point3D::new(
                start.x.min(end.x) - half_existing,
                start.y.min(end.y) - half_existing,
                start.z.min(end.z),
            ),
            max: Point3D::new(
                start.x.max(end.x) + half_existing,
                start.y.max(end.y) + half_existing,
                start.z.max(end.z),
            ),
        };

        // The inflation for new traces: use the NEW trace's half-width + clearance
        // This ensures the NEW trace routed alongside maintains proper spacing
        self.register_obstacle(
            bbox,
            new_trace_width_nm,
            clearance_nm,
            net_name,
            "Trace".into(),
        )
    }

    /// Register a via as an obstacle (blocks on all layers it passes through).
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate of via center.
    /// * `y_nm` - Y coordinate of via center.
    /// * `from_z_nm` - Bottom Z of via span.
    /// * `to_z_nm` - Top Z of via span.
    /// * `diameter_nm` - Via drill diameter.
    /// * `annular_ring_nm` - Copper pad around the drill hole.
    /// * `trace_width_nm` - Width of trace being routed (for inflation).
    /// * `clearance_nm` - Clearance for inflation.
    /// * `net_name` - Net name.
    pub fn register_via(
        &mut self,
        x_nm: i64,
        y_nm: i64,
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        annular_ring_nm: i64,
        trace_width_nm: i64,
        clearance_nm: i64,
        net_name: CompactString,
    ) -> i64 {
        let radius = diameter_nm / 2 + annular_ring_nm;
        let bbox = BoundingBox {
            min: Point3D::new(x_nm - radius, y_nm - radius, from_z_nm.min(to_z_nm)),
            max: Point3D::new(x_nm + radius, y_nm + radius, from_z_nm.max(to_z_nm)),
        };

        self.register_obstacle(bbox, trace_width_nm, clearance_nm, net_name, "Via".into())
    }

    /// Query all obstacles on a specific Z-layer.
    ///
    /// Returns obstacles whose layer_z_nm matches the given Z-height.
    /// This is used by the router to get obstacles for the active routing plane.
    ///
    /// # Arguments
    /// * `z_nm` - Z-height of the active layer in nanometers.
    ///
    /// # Returns
    /// Reference to vector of TrackedObstacle on this layer, or empty slice.
    pub fn query_layer(&self, z_nm: i64) -> &[TrackedObstacle] {
        self.by_layer
            .get(&z_nm)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Query all obstacles that intersect a given Z-range.
    ///
    /// This is used for via placement checks where an obstacle on any
    /// intermediate layer would block the via.
    ///
    /// # Arguments
    /// * `z_min_nm` - Minimum Z in nanometers.
    /// * `z_max_nm` - Maximum Z in nanometers.
    ///
    /// # Returns
    /// Vector of obstacles whose Z-range intersects the query range.
    pub fn query_z_range(&self, z_min_nm: i64, z_max_nm: i64) -> Vec<&TrackedObstacle> {
        self.all_obstacles
            .iter()
            .filter(|obs| {
                let obs_min = obs.original_bbox.min.z;
                let obs_max = obs.original_bbox.max.z;
                obs_min < z_max_nm && obs_max > z_min_nm
            })
            .collect()
    }

    /// Get the Minkowski-inflated bounding box for a specific query.
    ///
    /// This is a convenience method that performs the inflation calculation
    /// without registering an obstacle. Useful for hypothetical queries.
    ///
    /// # Arguments
    /// * `original_bbox` - The obstacle's original bounding box.
    /// * `trace_width_nm` - Width of the trace being routed.
    /// * `clearance_nm` - Minimum clearance to maintain.
    ///
    /// # Returns
    /// The Minkowski-inflated bounding box.
    pub fn compute_inflated_bbox(
        &self,
        original_bbox: &BoundingBox,
        trace_width_nm: i64,
        clearance_nm: i64,
    ) -> BoundingBox {
        let half_width = trace_width_nm / 2;
        let inflation_nm = half_width + clearance_nm;

        BoundingBox {
            min: Point3D::new(
                original_bbox.min.x - inflation_nm,
                original_bbox.min.y - inflation_nm,
                original_bbox.min.z,
            ),
            max: Point3D::new(
                original_bbox.max.x + inflation_nm,
                original_bbox.max.y + inflation_nm,
                original_bbox.max.z,
            ),
        }
    }

    /// Check if a point is inside any inflated obstacle on a given layer.
    ///
    /// Returns the first obstacle that contains the point, or None if clear.
    ///
    /// # Arguments
    /// * `point` - The point to check (nanometers).
    /// * `z_nm` - The Z-layer to check.
    ///
    /// # Returns
    /// Reference to the first containing obstacle, or None.
    pub fn point_collides(&self, point: Point3D, z_nm: i64) -> Option<&TrackedObstacle> {
        for obstacle in self.query_layer(z_nm) {
            if obstacle.inflated_bbox.contains(point) {
                return Some(obstacle);
            }
        }
        None
    }

    /// Check if a bounding box intersects any inflated obstacle on a given layer.
    ///
    /// Returns the first obstacle that intersects, or None if clear.
    ///
    /// # Arguments
    /// * `bbox` - The query bounding box (nanometers).
    /// * `z_nm` - The Z-layer to check.
    ///
    /// # Returns
    /// Reference to the first intersecting obstacle, or None.
    pub fn bbox_collides(&self, bbox: &BoundingBox, z_nm: i64) -> Option<&TrackedObstacle> {
        for obstacle in self.query_layer(z_nm) {
            if obstacle.inflated_bbox.intersects(bbox) {
                return Some(obstacle);
            }
        }
        None
    }

    /// Get all Minkowski-inflated AABBs on a given layer (for SDF registration).
    ///
    /// This is the primary integration point with the SDF generator. The
    /// pathfinder registers these inflated boxes and routes a zero-width ray,
    /// automatically satisfying all clearance constraints.
    ///
    /// # Arguments
    /// * `z_nm` - The Z-layer to query.
    ///
    /// # Returns
    /// Vector of (inflated_bbox, name) tuples for SDF registration.
    pub fn get_inflated_aabbs_for_sdf(&self, z_nm: i64) -> Vec<(BoundingBox, CompactString)> {
        self.query_layer(z_nm)
            .iter()
            .map(|obs| (obs.inflated_bbox, obs.name.clone()))
            .collect()
    }

    /// Clear all registered obstacles.
    ///
    /// Used when re-building the obstacle map for incremental compilation.
    pub fn clear(&mut self) {
        self.by_layer.clear();
        self.all_obstacles.clear();
        self.count = 0;
    }

    /// Get the total number of registered obstacles.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if the tracker is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get the number of unique layers with obstacles.
    pub fn layer_count(&self) -> usize {
        self.by_layer.len()
    }
}

impl Default for BoundingBoxTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic Minkowski inflation calculation.
    ///
    /// Given a component at (10mm, 10mm) with size (2mm x 4mm),
    /// trace width 0.2mm, clearance 0.15mm:
    ///
    /// Inflation = 0.2mm/2 + 0.15mm = 0.25mm
    ///
    /// Expected inflated bbox:
    /// - min.x = 10mm - 0.25mm = 9.75mm → 9_750_000nm
    /// - max.x = 12mm + 0.25mm = 12.25mm → 12_250_000nm
    /// - min.y = 10mm - 0.25mm = 9.75mm → 9_750_000nm
    /// - max.y = 14mm + 0.25mm = 14.25mm → 14_250_000nm
    /// - Z is unchanged (planar routing)
    #[test]
    fn test_minkowski_inflation_basic() {
        let mut tracker = BoundingBoxTracker::new();

        let bbox = BoundingBox::new(
            Point3D::new(10_000_000, 10_000_000, 500_000),
            Point3D::new(12_000_000, 14_000_000, 500_000),
        );

        let inflation = tracker.register_obstacle(
            bbox,
            200_000, // trace_width_nm = 0.2mm
            150_000, // clearance_nm = 0.15mm
            "U1".into(),
            "Component".into(),
        );

        // Inflation = 200_000/2 + 150_000 = 250_000
        assert_eq!(inflation, 250_000);

        let obstacles = tracker.query_layer(500_000);
        assert_eq!(obstacles.len(), 1);

        let inflated = &obstacles[0].inflated_bbox;
        assert_eq!(inflated.min.x, 9_750_000);  // 10mm - 0.25mm
        assert_eq!(inflated.max.x, 12_250_000);  // 12mm + 0.25mm
        assert_eq!(inflated.min.y, 9_750_000);   // 10mm - 0.25mm
        assert_eq!(inflated.max.y, 14_250_000);  // 14mm + 0.25mm

        // Z should be unchanged (planar routing)
        assert_eq!(inflated.min.z, 500_000);
        assert_eq!(inflated.max.z, 500_000);
    }

    /// Test that multiple layers are tracked independently.
    #[test]
    fn test_multi_layer_obstacles() {
        let mut tracker = BoundingBoxTracker::new();

        // Top layer obstacle
        tracker.register_obstacle(
            BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(1_000_000, 1_000_000, 0),
            ),
            200_000,
            150_000,
            "TOP_OBSTACLE".into(),
            "Component".into(),
        );

        // Bottom layer obstacle
        tracker.register_obstacle(
            BoundingBox::new(
                Point3D::new(0, 0, 2_000_000),
                Point3D::new(1_000_000, 1_000_000, 2_000_000),
            ),
            200_000,
            150_000,
            "BOTTOM_OBSTACLE".into(),
            "Component".into(),
        );

        // Query top layer should only return top obstacle
        let top_obstacles = tracker.query_layer(0);
        assert_eq!(top_obstacles.len(), 1);
        assert_eq!(top_obstacles[0].name.as_str(), "TOP_OBSTACLE");

        // Query bottom layer should only return bottom obstacle
        let bottom_obstacles = tracker.query_layer(2_000_000);
        assert_eq!(bottom_obstacles.len(), 1);
        assert_eq!(bottom_obstacles[0].name.as_str(), "BOTTOM_OBSTACLE");

        // Query non-existent layer should return empty
        let empty = tracker.query_layer(1_000_000);
        assert!(empty.is_empty());

        // Total count should be 2
        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.layer_count(), 2);
    }

    /// Test point collision detection with inflated AABBs.
    #[test]
    fn test_point_collision() {
        let mut tracker = BoundingBoxTracker::new();

        // Obstacle at (5mm, 5mm) size (1mm x 1mm)
        let bbox = BoundingBox::new(
            Point3D::new(5_000_000, 5_000_000, 0),
            Point3D::new(6_000_000, 6_000_000, 0),
        );

        tracker.register_obstacle(
            bbox,
            200_000, // trace_width_nm
            100_000, // clearance_nm
            "R1".into(),
            "Resistor".into(),
        );

        // Inflation = 200_000/2 + 100_000 = 200_000nm
        // Inflated bbox: min=(4_800_000, 4_800_000), max=(6_200_000, 6_200_000)

        // Point inside inflated bbox (should collide)
        let inside = Point3D::new(5_500_000, 5_500_000, 0);
        assert!(tracker.point_collides(inside, 0).is_some());

        // Point at edge of inflated bbox (should collide)
        let edge = Point3D::new(6_200_000, 5_500_000, 0);
        assert!(tracker.point_collides(edge, 0).is_some());

        // Point outside inflated bbox (should NOT collide)
        let outside = Point3D::new(6_300_000, 5_500_000, 0);
        assert!(tracker.point_collides(outside, 0).is_none());

        // Point on different layer (should NOT collide)
        let different_layer = Point3D::new(5_500_000, 5_500_000, 1_000_000);
        assert!(tracker.point_collides(different_layer, 1_000_000).is_none());
    }

    /// Test bounding box collision detection.
    #[test]
    fn test_bbox_collision() {
        let mut tracker = BoundingBoxTracker::new();

        // Obstacle spanning a Z-range (e.g., a component with thickness)
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(1_000_000, 1_000_000, 100_000),
        );

        tracker.register_obstacle(bbox, 200_000, 100_000, "C1".into(), "Capacitor".into());

        // Inflation = 200_000/2 + 100_000 = 200_000nm
        // Inflated bbox: min=(-200_000, -200_000, 0), max=(1_200_000, 1_200_000, 100_000)

        // Query bbox that overlaps inflated obstacle (min corner inside inflated region in XY)
        let overlapping = BoundingBox::new(
            Point3D::new(1_100_000, 1_100_000, 0),
            Point3D::new(2_000_000, 2_000_000, 50_000),
        );
        assert!(
            tracker.bbox_collides(&overlapping, 0).is_some(),
            "Overlapping bbox should collide (min corner inside inflated region in XY)"
        );

        // Query bbox that does NOT overlap (min is beyond inflated max in XY)
        let far = BoundingBox::new(
            Point3D::new(1_300_000, 1_300_000, 0),
            Point3D::new(2_000_000, 2_000_000, 50_000),
        );
        assert!(
            tracker.bbox_collides(&far, 0).is_none(),
            "Far bbox should not collide"
        );

        // Query bbox on a different Z layer should NOT collide
        let different_z = BoundingBox::new(
            Point3D::new(500_000, 500_000, 200_000),
            Point3D::new(600_000, 600_000, 300_000),
        );
        assert!(
            tracker.bbox_collides(&different_z, 200_000).is_none(),
            "Different layer bbox should not collide"
        );
    }

    /// Test trace registration as obstacle.
    #[test]
    fn test_trace_registration() {
        let mut tracker = BoundingBoxTracker::new();

        // Register a trace from (0,0) to (10mm, 0) with width 0.2mm
        let trace_start = Point3D::new(0, 0, 0);
        let trace_end = Point3D::new(10_000_000, 0, 0);

        tracker.register_trace(
            trace_start,
            trace_end,
            200_000, // existing trace width
            200_000, // new trace width
            150_000, // clearance
            "NET_VCC".into(),
        );

        let obstacles = tracker.query_layer(0);
        assert_eq!(obstacles.len(), 1);

        let obs = &obstacles[0];
        assert_eq!(obs.name.as_str(), "NET_VCC");
        assert_eq!(obs.obstacle_type.as_str(), "Trace");

        // The original bbox should include the trace's own half-width
        // half_existing = 200_000/2 = 100_000
        // original min.x = min(0, 10_000_000) - 100_000 = -100_000
        assert_eq!(obs.original_bbox.min.x, -100_000);
        assert_eq!(obs.original_bbox.max.x, 10_100_000);

        // Inflation = 200_000/2 + 150_000 = 250_000
        // inflated min.x = -100_000 - 250_000 = -350_000
        assert_eq!(obs.inflated_bbox.min.x, -350_000);
        assert_eq!(obs.inflated_bbox.max.x, 10_350_000);
    }

    /// Test via registration as obstacle.
    #[test]
    fn test_via_registration() {
        let mut tracker = BoundingBoxTracker::new();

        // Register a via at (5mm, 5mm) spanning z=0 to z=1mm
        // diameter = 0.3mm, annular ring = 0.1mm
        tracker.register_via(
            5_000_000,   // x_nm
            5_000_000,   // y_nm
            0,           // from_z_nm
            1_000_000,   // to_z_nm
            300_000,     // diameter_nm
            100_000,     // annular_ring_nm
            200_000,     // trace_width_nm
            150_000,     // clearance_nm
            "NET_VIA".into(),
        );

        // Via radius = 300_000/2 + 100_000 = 250_000
        // So via sits at x: 4_750_000 to 5_250_000 (before inflation)
        // Then inflation = 200_000/2 + 150_000 = 250_000
        // So inflated goes to x: 4_750_000 - 250_000 = 4_500_000

        let obstacles = tracker.query_layer(0);
        assert_eq!(obstacles.len(), 1);
        assert_eq!(obstacles[0].name.as_str(), "NET_VIA");
        assert_eq!(obstacles[0].obstacle_type.as_str(), "Via");

        // Via spans multiple layers - should be findable on multiple Z queries
        let z_range_obs = tracker.query_z_range(0, 1_000_000);
        assert_eq!(z_range_obs.len(), 1);
    }

    /// Test inflated AABBs retrieval for SDF registration.
    #[test]
    fn test_inflated_aabbs_for_sdf() {
        let mut tracker = BoundingBoxTracker::new();

        tracker.register_obstacle(
            BoundingBox::new(
                Point3D::new(1_000_000, 1_000_000, 0),
                Point3D::new(2_000_000, 2_000_000, 0),
            ),
            200_000,
            100_000,
            "U1".into(),
            "IC".into(),
        );

        tracker.register_obstacle(
            BoundingBox::new(
                Point3D::new(3_000_000, 3_000_000, 0),
                Point3D::new(4_000_000, 4_000_000, 0),
            ),
            200_000,
            100_000,
            "R1".into(),
            "Resistor".into(),
        );

        let aabbs = tracker.get_inflated_aabbs_for_sdf(0);
        assert_eq!(aabbs.len(), 2);

        // These AABBs can be directly passed to SdfGenerator::register_obstacle_bbox
        // The SDF will then calculate distances to the INFLATED boundaries
        // So a "zero-width" ray that stays outside these AABBs automatically
        // satisfies all clearance constraints.
    }

    /// Test that `compute_inflated_bbox` works correctly without registration.
    #[test]
    fn test_compute_inflated_bbox() {
        let tracker = BoundingBoxTracker::new();

        let original = BoundingBox::new(
            Point3D::new(10_000_000, 10_000_000, 500_000),
            Point3D::new(20_000_000, 20_000_000, 500_000),
        );

        let inflated = tracker.compute_inflated_bbox(&original, 400_000, 200_000);

        // Inflation = 400_000/2 + 200_000 = 400_000
        assert_eq!(inflated.min.x, 9_600_000);  // 10mm - 0.4mm
        assert_eq!(inflated.max.x, 20_400_000); // 20mm + 0.4mm
        assert_eq!(inflated.min.y, 9_600_000);
        assert_eq!(inflated.max.y, 20_400_000);
        // Z unchanged
        assert_eq!(inflated.min.z, 500_000);
        assert_eq!(inflated.max.z, 500_000);
    }

    /// Test that clear() resets the tracker.
    #[test]
    fn test_clear() {
        let mut tracker = BoundingBoxTracker::new();

        tracker.register_obstacle(
            BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(1_000_000, 1_000_000, 0),
            ),
            200_000,
            100_000,
            "D1".into(),
            "Diode".into(),
        );

        assert_eq!(tracker.len(), 1);
        assert!(!tracker.is_empty());

        tracker.clear();

        assert_eq!(tracker.len(), 0);
        assert!(tracker.is_empty());
        assert_eq!(tracker.layer_count(), 0);
    }

    /// Test that Z-range query works for vertical obstacles like vias.
    #[test]
    fn test_z_range_query() {
        let mut tracker = BoundingBoxTracker::new();

        // Short via on top layers (z=0 to z=100um)
        tracker.register_via(
            1_000_000, 1_000_000,
            0, 100_000,
            200_000, 50_000,
            200_000, 100_000,
            "VIA_TOP".into(),
        );

        // Tall via through whole board (z=0 to z=1mm)
        tracker.register_via(
            2_000_000, 2_000_000,
            0, 1_000_000,
            300_000, 100_000,
            200_000, 100_000,
            "VIA_THROUGH".into(),
        );

        // Query only middle layers (z=200um to z=800um)
        let middle = tracker.query_z_range(200_000, 800_000);
        assert_eq!(middle.len(), 1, "Only the through-hole via should intersect");
        assert_eq!(middle[0].name.as_str(), "VIA_THROUGH");

        // Query all layers
        let all = tracker.query_z_range(0, 1_000_000);
        assert_eq!(all.len(), 2, "Both vias should intersect full range");
    }
}