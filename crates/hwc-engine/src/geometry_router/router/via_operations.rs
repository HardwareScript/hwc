//! Via-related operations: extraction, validation, stamping, clearing, and tower unrolling

use super::super::types::{Via, ViaType};
use super::core::GeometryRouter;
use crate::geometry::Point3D;
use crate::netlist::NetId;

impl GeometryRouter {
    /// Extract vias from a routed path by detecting Z changes.
    /// Coalesces consecutive Z-changes into single layer-transition vias
    /// instead of creating one via per Z-voxel boundary.
    pub(super) fn extract_vias_from_path(&self, path: &[Point3D], net_id: NetId) -> Vec<Via> {
        let mut vias = Vec::new();
        let board_min_z_nm = 0;
        let board_max_z_nm = self.bounds.depth_nm;

        if path.len() < 2 {
            return vias;
        }

        let mut i = 0;
        while i < path.len() - 1 {
            let current = path[i];

            // Find the start of a Z-change sequence
            if current.z == path[i + 1].z {
                i += 1;
                continue;
            }

            // We have a Z-change at index i. Find the end of this Z-change sequence
            // (consecutive points with the same Z after the transition).
            let from_z = current.z;
            let x = current.x;
            let y = current.y;
            let mut to_z = path[i + 1].z;

            // Scan forward: skip all intermediate Z-voxel steps until Z stabilizes
            let mut j = i + 1;
            while j < path.len() && path[j].z != from_z {
                to_z = path[j].z;
                // If next point is back at from_z, this is a transient Z-spike - skip it
                if j + 1 < path.len() && path[j + 1].z == from_z {
                    j += 1;
                    continue;
                }
                j += 1;
            }

            // Only create a via if we actually changed layers (not a transient spike)
            if to_z != from_z {
                let diameter_nm = self
                    .constraints
                    .fabrication
                    .as_ref()
                    .map(|f| f.min_via_diameter_nm)
                    .unwrap_or(300_000);

                let via = Via::new(
                    (x, y),
                    from_z.min(to_z),
                    from_z.max(to_z),
                    diameter_nm,
                    net_id,
                    board_min_z_nm,
                    board_max_z_nm,
                    self.voxel_size_nm,
                    self.constraints
                        .fabrication
                        .as_ref()
                        .map(|f| f.min_annular_ring_nm)
                        .unwrap_or(0),
                );

                vias.push(via);
            }

            i = j;
        }

        vias
    }

    /// Check if a via can be placed at the given position and Z span.
    pub(super) fn can_place_via(&self, position: (i64, i64), from_z_nm: i64, to_z_nm: i64) -> bool {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return true,
        };

        let via_diameter = fabrication.min_via_diameter_nm;
        let annular_ring = fabrication.min_annular_ring_nm;
        let clearance = fabrication.min_trace_spacing_nm;
        let total_radius = (via_diameter + 2 * annular_ring + clearance) / 2;

        let via = Via {
            position,
            from_z_nm,
            to_z_nm,
            diameter_nm: via_diameter,
            net_id: NetId::new(0),
            via_type: ViaType::ThroughHole,
            annular_ring_nm: annular_ring,
            properties: rustc_hash::FxHashMap::default(),
        };

        for z_nm in via.z_planes(self.voxel_size_nm) {
            if !self.is_circular_area_clear(position, total_radius, z_nm) {
                return false;
            }
        }

        true
    }

    /// Stamp via footprint on all Z planes it passes through.
    pub fn stamp_via(&mut self, via: &Via) {
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
    pub(super) fn generate_antipads(&mut self, via: &Via) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let clearance = fabrication.min_trace_spacing_nm;
        let antipad_radius = (via.diameter_nm + 2 * clearance) / 2;

        for z_nm in via.z_planes(self.voxel_size_nm) {
            for pour in &self.copper_pours.clone() {
                if pour.z_bottom_nm == z_nm {
                    if pour.net_id != via.net_id {
                        self.remove_circular_area(via.position, antipad_radius, z_nm);
                    } else if let Some(hwc_parser::Expression::Variable { name, .. }) =
                        via.properties.get("thermal_relief")
                    {
                        if name == "true" {
                            use crate::geometry_router::thermal_relief::{
                                ThermalReliefConfig, ThermalReliefGenerator,
                            };
                            let generator = ThermalReliefGenerator::new(
                                ThermalReliefConfig::default(),
                                self.voxel_size_nm,
                            );
                            let center = crate::geometry_router::polygon_rasterizer::Point2D::new(
                                via.position.0,
                                via.position.1,
                            );
                            generator.generate_for_circular_pad(
                                center,
                                via.diameter_nm / 2,
                                z_nm,
                                pour.material_id as crate::voxel_grid::MaterialId,
                                via.net_id.raw() as crate::voxel_grid::NetId,
                                &mut self.voxel_grid,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Clear a via (for rip-up and reroute).
    pub fn clear_via(&mut self, via: &Via) {
        self.vias.retain(|v| {
            v.position != via.position || v.from_z_nm != via.from_z_nm || v.to_z_nm != via.to_z_nm
        });

        for z_nm in via.z_planes(self.voxel_size_nm) {
            self.remove_circular_area(via.position, via.diameter_nm, z_nm);
        }
    }

    // =========================================================================
    // v0.1.7 Via Tower Unroller (ASIC Multi-Layer Transitions)
    // =========================================================================

    /// Unroll a multi-layer via transition into single-layer vias with intermediate landing pads.
    ///
    /// ASIC profiles forbid direct multi-layer via transitions (e.g., M1→M4).
    /// A vertical step must be unrolled into single-layer vias (M1→M2, M2→M3, M3→M4)
    /// with intermediate landing pads at each layer.
    ///
    /// For PCB (Octilinear) profiles, a single through-hole via spanning the full
    /// depth is emitted instead.
    ///
    /// **Architecture Reference:** `ROADMAP/v0.1.7/ADVANCED-ROUTING-IMPLEMENTATION.md` List 3
    ///
    /// # Arguments
    /// * `pos` - XY position of the via tower
    /// * `start_layer_idx` - Index of the starting layer in `profile_layers`
    /// * `end_layer_idx` - Index of the ending layer in `profile_layers`
    /// * `profile_layers` - Ordered layer names from the stackup profile (bottom-to-top)
    /// * `net_id` - Net ID this via belongs to
    /// * `is_manhattan` - True for ASIC (Manhattan angle restriction), false for PCB (Octilinear)
    ///
    /// # Returns
    /// Vector of `Via` objects representing the unrolled tower.
    pub fn unroll_via_tower(
        &self,
        pos: (i64, i64),
        start_layer_idx: usize,
        end_layer_idx: usize,
        _profile_layers: &[String],
        net_id: NetId,
        is_manhattan: bool,
    ) -> Vec<Via> {
        let mut via_tower = Vec::new();
        let diameter_nm = self
            .constraints
            .fabrication
            .as_ref()
            .map(|f| f.min_via_diameter_nm)
            .unwrap_or(300_000);

        let annular_ring = self
            .constraints
            .fabrication
            .as_ref()
            .map(|f| f.min_annular_ring_nm)
            .unwrap_or(0);

        if is_manhattan {
            // ASIC: step one layer at a time, emit a Via per adjacent layer pair
            let step = if end_layer_idx > start_layer_idx {
                1isize
            } else {
                -1isize
            };
            let mut current_idx = start_layer_idx as isize;

            while current_idx != end_layer_idx as isize {
                let next_idx = (current_idx + step) as usize;

                // Use actual Z positions from the stackup (layer_z_positions)
                // instead of the incorrect `index * voxel_size_nm` calculation.
                let from_z = if (current_idx as usize) < self.layer_z_positions.len() {
                    self.layer_z_positions[current_idx as usize]
                } else {
                    current_idx as i64 * self.voxel_size_nm
                };

                let to_z = if next_idx < self.layer_z_positions.len() {
                    self.layer_z_positions[next_idx]
                } else {
                    next_idx as i64 * self.voxel_size_nm
                };

                via_tower.push(Via::new_with_type(
                    pos,
                    from_z,
                    to_z,
                    diameter_nm,
                    net_id,
                    ViaType::Buried, // Intermediate vias in ASIC towers are buried
                    annular_ring,
                ));

                current_idx = next_idx as isize;
            }
        } else {
            // PCB: emit a single through-hole via spanning the full depth
            let from_z = start_layer_idx as i64 * self.voxel_size_nm;
            let to_z = end_layer_idx as i64 * self.voxel_size_nm;

            via_tower.push(Via::new_with_type(
                pos,
                from_z,
                to_z,
                diameter_nm,
                net_id,
                ViaType::ThroughHole,
                annular_ring,
            ));
        }

        via_tower
    }

    /// Generate intermediate landing pads for an ASIC via tower.
    ///
    /// Each intermediate layer in the via tower needs a small copper landing pad
    /// (annular ring) to anchor the via physically. This method stamps copper
    /// discs at each intermediate Z using the VoxelGrid.
    ///
    /// # Arguments
    /// * `via_tower` - The unrolled via tower from `unroll_via_tower`
    /// * `enclosures` - Per-layer enclosure sizes (layer_name → enclosure_nm)
    pub fn generate_intermediate_landing_pads(
        &mut self,
        via_tower: &[Via],
        enclosures: &rustc_hash::FxHashMap<String, i64>,
    ) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let annular_ring = fabrication.min_annular_ring_nm;

        for via in via_tower {
            // For each intermediate Z plane (not the first or last), stamp a landing pad
            let z_planes = via.z_planes(self.voxel_size_nm);
            for (i, &z_nm) in z_planes.iter().enumerate() {
                if i == 0 || i == z_planes.len() - 1 {
                    continue; // Skip the start and end layers (already have pads from the via)
                }

                // Calculate landing pad radius: via_radius + enclosure
                let enclosure = enclosures.values().next().copied().unwrap_or(annular_ring);
                let pad_radius = (via.diameter_nm / 2) + enclosure;

                // Stamp the landing pad
                self.mark_circular_area_occupied(via.position, pad_radius, z_nm, via.net_id);
            }
        }
    }

    /// Find the layer index for a given Z position using the profile's layer Z positions.
    ///
    /// Returns the index of the layer whose Z range contains `z_nm`.
    /// If `z_nm` is between layers, returns the index of the closest layer below.
    pub fn find_layer_index_at_z(&self, z_nm: i64) -> Option<usize> {
        if self.layer_z_positions.is_empty() {
            return None;
        }
        // Find the last layer whose start Z is <= z_nm
        for (i, &layer_z) in self.layer_z_positions.iter().enumerate().rev() {
            if z_nm >= layer_z {
                return Some(i);
            }
        }
        // z_nm is below all layers, return first layer
        Some(0)
    }

    /// Unroll a detected via into layer-by-layer vias when in ASIC (Manhattan) mode.
    ///
    /// When `is_manhattan` is false, returns the original via unchanged.
    /// When `is_manhattan` is true, splits the via into individual buried vias
    /// for each adjacent layer pair it spans.
    pub fn unroll_detected_via(&self, via: &Via) -> Vec<Via> {
        if !self.is_manhattan || self.profile_layers.is_empty() || self.layer_z_positions.is_empty()
        {
            return vec![via.clone()];
        }

        let min_z = via.from_z_nm.min(via.to_z_nm);
        let max_z = via.from_z_nm.max(via.to_z_nm);

        let start_idx = self.find_layer_index_at_z(min_z).unwrap_or(0);
        let end_idx = self
            .find_layer_index_at_z(max_z)
            .unwrap_or(self.profile_layers.len() - 1);

        // If the via only spans one layer, no unrolling needed
        if start_idx == end_idx {
            return vec![via.clone()];
        }

        self.unroll_via_tower(
            via.position,
            start_idx,
            end_idx,
            &self.profile_layers,
            via.net_id,
            self.is_manhattan,
        )
    }
}
