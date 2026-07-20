//! Minkowski obstacle inflation integration

use super::types::GeometryRouter;
use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::bounding_box_tracker::BoundingBoxTracker;

impl GeometryRouter {
    /// Register a component obstacle with Minkowski inflation.
    pub fn add_minkowski_obstacle(
        &mut self,
        bbox: BoundingBox,
        trace_width_nm: i64,
        clearance_nm: i64,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
    ) -> i64 {
        self.bounding_box_tracker.register_component(
            bbox,
            trace_width_nm,
            clearance_nm,
            name,
            component_type,
        )
    }

    /// Register a previously routed trace as an obstacle.
    pub fn add_minkowski_trace(
        &mut self,
        start: Point3D,
        end: Point3D,
        existing_trace_width_nm: i64,
        new_trace_width_nm: i64,
        clearance_nm: i64,
        net_name: compact_str::CompactString,
    ) -> i64 {
        self.bounding_box_tracker.register_trace(
            start,
            end,
            existing_trace_width_nm,
            new_trace_width_nm,
            clearance_nm,
            net_name,
        )
    }

    /// Get a reference to the BoundingBoxTracker for SDF generation.
    pub fn get_bounding_box_tracker(&self) -> &BoundingBoxTracker {
        &self.bounding_box_tracker
    }

    /// Clear all Minkowski-inflated obstacles for incremental compilation.
    pub fn clear_bounding_box_tracker(&mut self) {
        self.bounding_box_tracker.clear();
    }
}
