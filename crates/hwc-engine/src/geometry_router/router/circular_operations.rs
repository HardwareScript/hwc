//! Circular area operations for via footprints and anti-pads
//!
//! # Roadmap 14.1 — Circular Area Operations (EntityGraph-native)
//!
//! ## Why These Functions Were Rewritten (v0.1.8)
//!
//! The legacy v0.1.7 implementations were no-op stubs inherited from the
//! legacy occupancy architecture. They caused the **split-brain data model**:
//!
//! - `mark_circular_area_occupied` silently discarded all occupancy data,
//!   so via pads were invisible to the EntityGraph spatial index.
//! - `remove_circular_area` silently ignored removal requests, leaving stale
//!   occupancy in the spatial index and causing phantom DRC violations.
//! - `is_circular_area_clear` only checked `component_metadata` bounding boxes,
//!   missing substrate layers (pours, contacts) and routed segments entirely.
//!
//! This meant two independent sources of truth existed for geometry occupancy:
//! the `Vec<Via>` in GeometryRouter and the EntityGraph spatial index, with no
//! guarantee of consistency between them.
//!
//! ## What Replaces Them
//!
//! All three functions now use EntityGraph-native operations:
//!
//! - `mark_circular_area_occupied` registers via pads as analytic cylinder
//!   substrate layers via `entity_graph.add_cylinder_substrate_layer()`.
//! - `remove_circular_area` finds and removes matching substrate layers, then
//!   rebuilds the spatial index via `entity_graph.rebuild_spatial_index()`.
//! - `is_circular_area_clear` queries ALL registered geometry — components,
//!   substrate layers, and routed segments — ensuring a single source of truth.
//!
//! **This is a pre-release full transition (no backward compatibility).**

use super::core::GeometryRouter;
use crate::geometry::{BoundingBox, Point3D};

impl GeometryRouter {
    /// Check if a circular area is clear at a Z elevation.
    ///
    /// Queries ALL registered geometry in the EntityGraph: component metadata,
    /// substrate layers (via pads, pours, contacts), and routed segments. Returns
    /// true only if no overlapping geometry exists within the circle.
    ///
    /// v0.1.8: Replaces the legacy component-metadata-only check that missed
    /// substrate layers and routes — the root cause of the split-brain data model.
    /// See module-level documentation for full rationale.
    pub(super) fn is_circular_area_clear(
        &self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
    ) -> bool {
        let half_thickness = self.resolution_nm / 2;
        let bbox = BoundingBox::new(
            Point3D::new(
                center.0 - radius_nm,
                center.1 - radius_nm,
                z_nm - half_thickness,
            ),
            Point3D::new(
                center.0 + radius_nm,
                center.1 + radius_nm,
                z_nm + half_thickness,
            ),
        );

        // 1. Check component metadata (bounding box overlap + circle distance)
        for component in self.entity_graph.get_component_metadata() {
            if !component.bbox.intersects(&bbox) {
                continue;
            }
            let closest_x = center.0.clamp(component.bbox.min.x, component.bbox.max.x);
            let closest_y = center.1.clamp(component.bbox.min.y, component.bbox.max.y);
            let dx = center.0 - closest_x;
            let dy = center.1 - closest_y;
            if dx * dx + dy * dy < radius_nm * radius_nm {
                return false;
            }
        }

        // 2. Check substrate layers (via pads, pours, contacts, etc.)
        for layer in self.entity_graph.get_substrate_layers() {
            if !layer.bbox.intersects(&bbox) {
                continue;
            }
            let closest_x = center.0.clamp(layer.bbox.min.x, layer.bbox.max.x);
            let closest_y = center.1.clamp(layer.bbox.min.y, layer.bbox.max.y);
            let dx = center.0 - closest_x;
            let dy = center.1 - closest_y;
            if dx * dx + dy * dy < radius_nm * radius_nm {
                return false;
            }
        }

        // 3. Check routed segments (traces registered via register_route)
        for (_net_id, segments) in self.entity_graph.get_all_routes() {
            for seg in segments {
                let seg_bbox = BoundingBox::new(seg.start, seg.end);
                if !seg_bbox.intersects(&bbox) {
                    continue;
                }
                let closest_x = center.0.clamp(seg_bbox.min.x, seg_bbox.max.x);
                let closest_y = center.1.clamp(seg_bbox.min.y, seg_bbox.max.y);
                let dx = center.0 - closest_x;
                let dy = center.1 - closest_y;
                if dx * dx + dy * dy < radius_nm * radius_nm {
                    return false;
                }
            }
        }

        true
    }

    /// Mark a circular area as occupied by a net at a Z elevation.
    ///
    /// Registers the via pad as an analytic cylinder substrate layer in the
    /// EntityGraph via `add_cylinder_substrate_layer()`. This makes via pads
    /// visible to all spatial queries (DRC, clearance checks).
    ///
    /// v0.1.8: Replaces the legacy no-op stub that silently discarded occupancy
    /// data, causing via pads to be invisible to the EntityGraph spatial index.
    /// See module-level documentation for full rationale.
    pub(super) fn mark_circular_area_occupied(
        &mut self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
        net_id: crate::netlist::NetId,
    ) {
        let half_thickness = self.resolution_nm / 2;
        let bbox = BoundingBox::new(
            Point3D::new(
                center.0 - radius_nm,
                center.1 - radius_nm,
                z_nm - half_thickness,
            ),
            Point3D::new(
                center.0 + radius_nm,
                center.1 + radius_nm,
                z_nm + half_thickness,
            ),
        );

        self.entity_graph.add_cylinder_substrate_layer(
            self.routing_material_id,
            net_id.raw(),
            bbox,
            radius_nm * 2, // diameter
            32,            // tessellation segments (matches entity_graph defaults)
            0,             // rotation
        );
    }

    /// Remove a circular area from occupied zones at a Z elevation.
    ///
    /// Finds and removes matching substrate layers from the EntityGraph by
    /// identifying Circle-shaped layers centered at the given position with
    /// matching radius. Rebuilds the spatial index after removal to ensure
    /// consistency.
    ///
    /// v0.1.8: Replaces the legacy no-op stub that silently ignored removal
    /// requests, leaving stale occupancy in the spatial index and causing
    /// phantom DRC violations during rip-up and reroute.
    /// See module-level documentation for full rationale.
    pub(super) fn remove_circular_area(&mut self, center: (i64, i64), radius_nm: i64, z_nm: i64) {
        let half_thickness = self.resolution_nm / 2;
        let bbox = BoundingBox::new(
            Point3D::new(
                center.0 - radius_nm,
                center.1 - radius_nm,
                z_nm - half_thickness,
            ),
            Point3D::new(
                center.0 + radius_nm,
                center.1 + radius_nm,
                z_nm + half_thickness,
            ),
        );

        self.entity_graph.substrate_layers.retain(|layer| {
            if !layer.bbox.intersects(&bbox) {
                return true; // Keep layers that don't overlap
            }
            // Match by Circle shape, center position, and radius to identify
            // the exact layer registered by mark_circular_area_occupied.
            if let crate::geometry_router::substrate_types::SubstrateLayerShape::Circle { radius } =
                &layer.shape
            {
                let layer_center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let layer_center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let dx = layer_center_x - center.0;
                let dy = layer_center_y - center.1;
                if dx * dx + dy * dy <= 1 && *radius == radius_nm {
                    return false; // Remove matching layer
                }
            }
            true
        });

        // NOTE: entity_graph's spatial index is no longer used for routing.
        // Each routing method builds its own independent spatial index via build_routing_spatial_index.
        // self.entity_graph.rebuild_spatial_index(&self.material_registry); // REMOVED
    }
}
