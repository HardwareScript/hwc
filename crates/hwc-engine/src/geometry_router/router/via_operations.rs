//! Via-related operations: extraction, validation, stamping, and clearing

use super::core::GeometryRouter;

impl GeometryRouter {
    /// Extract vias from a routed path by detecting Z changes.
    pub(super) fn extract_vias_from_path(
        &self,
        path: &[crate::geometry::Point3D],
        net_id: crate::netlist::NetId,
    ) -> Vec<super::super::types::Via> {
        use super::super::types::Via;

        let mut vias = Vec::new();
        let board_min_z_nm = 0;
        let board_max_z_nm = self.bounds.depth_nm;

        for i in 0..path.len().saturating_sub(1) {
            let current = path[i];
            let next = path[i + 1];

            if current.z != next.z {
                let diameter_nm = self
                    .constraints
                    .fabrication
                    .as_ref()
                    .map(|f| f.min_via_diameter_nm)
                    .unwrap_or(300_000);

                let via = Via::new(
                    (current.x, current.y),
                    current.z,
                    next.z,
                    diameter_nm,
                    net_id,
                    board_min_z_nm,
                    board_max_z_nm,
                    self.voxel_size_nm,
                );

                vias.push(via);
            }
        }

        vias
    }

    /// Check if a via can be placed at the given position and Z span.
    pub(super) fn can_place_via(
        &self,
        position: (i64, i64),
        from_z_nm: i64,
        to_z_nm: i64,
    ) -> bool {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return true,
        };

        let via_diameter = fabrication.min_via_diameter_nm;
        let annular_ring = fabrication.min_annular_ring_nm;
        let clearance = fabrication.min_trace_spacing_nm;
        let total_radius = (via_diameter + 2 * annular_ring + clearance) / 2;

        let via = super::super::types::Via {
            position,
            from_z_nm,
            to_z_nm,
            diameter_nm: via_diameter,
            net_id: crate::netlist::NetId::new(0),
            via_type: super::super::types::ViaType::ThroughHole,
        };

        for z_nm in via.z_planes(self.voxel_size_nm) {
            if !self.is_circular_area_clear(position, total_radius, z_nm) {
                return false;
            }
        }

        true
    }

    /// Stamp via footprint on all Z planes it passes through.
    pub fn stamp_via(&mut self, via: &super::super::types::Via) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let annular_ring = fabrication.min_annular_ring_nm;
        let total_radius = (via.diameter_nm + 2 * annular_ring) / 2;

        for z_nm in via.z_planes(self.voxel_size_nm) {
            self.mark_circular_area_occupied(via.position, total_radius, z_nm, via.net_id);
        }

        self.generate_antipads(via);
    }

    /// Generate anti-pads for vias passing through copper pours on different nets.
    pub(super) fn generate_antipads(&mut self, via: &super::super::types::Via) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        // v0.1.7 ARCHITECTURE: Anti-pad clearance comes from the profile's trace.min_spacing.
        // high_voltage_clearance_nm (1.5–3mm) is reserved for HV isolation and must NOT
        // govern the physical gap between a via and an adjacent copper pour on a normal PCB.
        let clearance = fabrication.min_trace_spacing_nm;
        let antipad_radius = (via.diameter_nm + 2 * clearance) / 2;

        for z_nm in via.z_planes(self.voxel_size_nm) {
            for pour in &self.copper_pours.clone() {
                if pour.z_bottom_nm == z_nm && pour.net_id != via.net_id {
                    self.remove_circular_area(via.position, antipad_radius, z_nm);
                }
            }
        }
    }

    /// Clear a via (for rip-up and reroute).
    pub fn clear_via(&mut self, via: &super::super::types::Via) {
        self.vias.retain(|v| {
            v.position != via.position
                || v.from_z_nm != via.from_z_nm
                || v.to_z_nm != via.to_z_nm
        });

        for z_nm in via.z_planes(self.voxel_size_nm) {
            self.remove_circular_area(via.position, via.diameter_nm, z_nm);
        }
    }
}
