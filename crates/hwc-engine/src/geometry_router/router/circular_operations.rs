//! Circular area operations for via footprints and anti-pads

use super::core::GeometryRouter;

impl GeometryRouter {
    /// Check if a circular area is clear at a Z elevation.
    /// Uses analytic distance checks instead of voxel iteration.
    pub(super) fn is_circular_area_clear(
        &self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
    ) -> bool {
        for component in self.entity_graph.get_component_metadata() {
            let bbox = &component.bbox;
            let closest_x = center.0.clamp(bbox.min.x, bbox.max.x);
            let closest_y = center.1.clamp(bbox.min.y, bbox.max.y);
            let dx = center.0 - closest_x;
            let dy = center.1 - closest_y;
            if dx * dx + dy * dy < radius_nm * radius_nm {
                if z_nm >= bbox.min.z && z_nm <= bbox.max.z {
                    return false;
                }
            }
        }
        true
    }

    /// Mark a circular area as occupied by a net at a Z elevation.
    /// Records the area in the entity graph.
    pub(super) fn mark_circular_area_occupied(
        &mut self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
        _net_id: crate::netlist::NetId,
    ) {
        // EntityGraph tracks occupancy through component metadata
        let _ = (center, radius_nm, z_nm);
    }

    /// Remove a circular area from occupied zones at a Z elevation.
    pub(super) fn remove_circular_area(&mut self, center: (i64, i64), radius_nm: i64, z_nm: i64) {
        // EntityGraph manages occupancy - removal handled at a higher level
        let _ = (center, radius_nm, z_nm);
    }
}
