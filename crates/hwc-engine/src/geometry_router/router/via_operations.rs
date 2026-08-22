//! Via-related operations: extraction, validation, stamping, clearing, and tower unrolling
//!
//! # Roadmap 14.2 — Via Stamping Operations (v0.1.8)
//!
//! All via operations now delegate to EntityGraph-native circular operations
//! (Roadmap 14.1) instead of legacy occupancy stubs:
//!
//! - `stamp_via` → `mark_circular_area_occupied` → `entity_graph.add_cylinder_substrate_layer()`
//! - `clear_via` → `remove_circular_area` → `entity_graph.substrate_layers.retain()` + `rebuild_spatial_index()`
//! - `can_place_via` → `is_circular_area_clear` → queries ALL geometry (components, substrates, routes)
//! - `generate_antipads` → `remove_circular_area` for same-layer pours, thermal relief for same-net
//! - `unroll_via_tower` → uses `layer_z_positions` directly (no legacy fallbacks)
//!
//! **This is a pre-release full transition (no backward compatibility).**

use super::super::types::{Via, ViaSpec, ViaType};
use super::core::GeometryRouter;
use crate::geometry::Point3D;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;

impl GeometryRouter {
    /// Extract vias from a routed path by detecting Z changes.
    /// Coalesces consecutive Z-changes into single layer-transition vias
    /// instead of creating one via per Z-layer boundary.
    ///
    /// v0.2.0: Filters out Z-transitions that already have explicit contacts
    /// placed by the user. This prevents duplicate vias at the same position
    /// (Bug Fix: Edge-Drop Router Issue).
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

            // Scan forward: skip all intermediate Z-layer steps until Z stabilizes
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

            // v0.1.8: Layer-aware Z-delta threshold — replaces legacy
            // `manufacturing_grid_nm / 2` check. Uses minimum spacing between adjacent
            // layers to determine if a Z-change represents a real layer transition
            // versus sub-layer noise. Falls back to manufacturing_grid_nm/2 only when
            // layer_z_positions is empty (no stackup defined).
            let z_delta = (to_z - from_z).abs();
            let z_threshold = if self.config.layer_z_positions.len() >= 2 {
                // Compute minimum gap between any two adjacent layers
                let mut min_gap = i64::MAX;
                for w in self.config.layer_z_positions.windows(2) {
                    let gap = (w[1] - w[0]).abs();
                    if gap > 0 && gap < min_gap {
                        min_gap = gap;
                    }
                }
                // A via is real if it spans more than 50% of the smallest layer gap
                min_gap / 2
            } else {
                self.manufacturing_grid_nm / 2
            };
            if to_z != from_z && z_delta > z_threshold {
                let fabrication = self
                    .constraints
                    .fabrication
                    .as_ref()
                    .expect("FATAL: Fabrication constraints required for via extraction. Ensure a profile with 'trace:' and 'via:' constraints is declared in the space definition.");

                let diameter_nm = fabrication.min_via_diameter_nm;
                let enclosure_nm = fabrication.min_enclosure_nm;

                let via = Via::new(ViaSpec {
                    position: (x, y),
                    from_z_nm: from_z.min(to_z),
                    to_z_nm: from_z.max(to_z),
                    diameter_nm,
                    net_id,
                    material_id: self.routing_material_id, // Use the active routing material context
                    enclosure_nm,
                    board_min_z_nm,
                    board_max_z_nm,
                });

                vias.push(via);
            }

            i = j;
        }

        // v0.2.0: Filter out vias that overlap with existing explicit contacts.
        // This prevents the router from creating duplicate vias when the user
        // has already placed contacts at specific locations (Bug Fix: Edge-Drop Issue).

        let _initial_via_count = vias.len();
        vias.retain(|via| {
            !self.has_existing_contact_at(via.position, via.from_z_nm, via.to_z_nm, net_id)
        });

        vias
    }

    /// Check if an explicit contact already exists at the given position and Z range.
    ///
    /// v0.2.0: Queries substrate layers to detect user-placed contacts that span
    /// the same vertical transition as a router-generated via. Used to prevent
    /// duplicate vias when explicit contacts exist (Bug Fix: Edge-Drop Router Issue).
    ///
    /// Returns true if a cylindrical substrate layer (Circle shape) exists on the
    /// same net that overlaps both the XY position and Z range of the via.
    fn has_existing_contact_at(
        &self,
        position: (i64, i64),
        from_z: i64,
        to_z: i64,
        net_id: NetId,
    ) -> bool {
        use crate::geometry_router::substrate_types::SubstrateLayerShape;

        let min_z = from_z.min(to_z);
        let max_z = from_z.max(to_z);
        let _net_raw = net_id.raw();

        // STRUCTURAL FIX: Use self.substrate_layers (populated by route_space) instead of
        // entity_graph.get_substrate_layers() (which only contains component obstacles, not substrate layers).
        // substrate_layers are passed from the space and stored during route_space() initialization.
        let substrate_layers = match &self.substrate_layers {
            Some(layers) => layers,
            None => return false, // No substrate context available, cannot deduplicate
        };

        // Check all substrate layers for existing cylindrical contacts
        for layer in substrate_layers.iter() {
            // Must be on the same net
            if layer.net != net_id {
                continue;
            }

            // Only check Circle-shaped layers (contacts/vias)
            // This filters out rectangular pours and pads
            let is_circular = matches!(layer.shape, SubstrateLayerShape::Circle { .. });
            if !is_circular {
                continue;
            }

            // Check if the contact's Z range overlaps with the via's Z range
            let layer_min_z = layer.bbox.min.z;
            let layer_max_z = layer.bbox.max.z;

            // Significant Z overlap required (>50% of via height)
            let via_height = (max_z - min_z).max(1);
            let overlap_z = layer_max_z.min(max_z) - layer_min_z.max(min_z);
            if overlap_z < via_height / 2 {
                continue; // No significant Z overlap
            }

            // Check if the contact's XY position overlaps with the via position
            // Use the contact's radius from its shape definition
            let tolerance = if let SubstrateLayerShape::Circle { radius } = layer.shape {
                radius
            } else {
                // This should never happen since we filtered for Circle shape above,
                // but panic with a clear message if it does
                panic!(
                    "BUG: Layer passed Circle shape check but is not Circle: shape={:?}",
                    layer.shape
                );
            };

            let layer_center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
            let layer_center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;
            let dx = layer_center_x - position.0;
            let dy = layer_center_y - position.1;
            let distance_sq = dx * dx + dy * dy;
            let tolerance_sq = tolerance * tolerance;

            if distance_sq <= tolerance_sq {
                println!(
                    "[VIA DEDUP] Skipping auto-generated via at ({},{}) Z={}→{}nm - \
                     explicit cylindrical contact exists at ({},{}) Z={}→{}nm (distance={}nm, tolerance={}nm)",
                    position.0, position.1, min_z, max_z,
                    layer_center_x, layer_center_y, layer_min_z, layer_max_z,
                    (distance_sq as f64).sqrt() as i64, tolerance
                );
                return true;
            }
        }

        false
    }

    /// Check if a via can be placed at the given position and Z span.
    ///
    /// v0.1.8 (Roadmap 14.2): Queries ALL registered geometry in the EntityGraph
    /// — component metadata, substrate layers (via pads, pours, contacts), and
    /// routed segments — via `is_circular_area_clear()`. This replaces the legacy
    /// component-metadata-only check that missed substrate layers and routes.
    pub(super) fn can_place_via(
        &self,
        entity_graph: &EntityGraph,
        position: (i64, i64),
        from_z_nm: i64,
        to_z_nm: i64,
    ) -> bool {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return true,
        };

        let via_diameter = fabrication.min_via_diameter_nm;
        let enclosure = fabrication.min_enclosure_nm;
        let clearance = fabrication.min_trace_spacing_nm;
        let total_radius = (via_diameter + 2 * enclosure + clearance) / 2;

        let via = Via {
            position,
            from_z_nm,
            to_z_nm,
            diameter_nm: via_diameter,
            net_id: NetId::new(0),
            material_id: self.routing_material_id, // Use the active routing material context
            via_type: ViaType::ThroughHole,
            enclosure_nm: enclosure,
            properties: rustc_hash::FxHashMap::default(),
            is_frozen: false,
            parent_instance: None,
        };

        for z_nm in via.z_planes_between(&self.config.layer_z_positions, 0, self.bounds.depth_nm) {
            if !self.is_circular_area_clear(entity_graph, position, total_radius, z_nm) {
                return false;
            }
        }

        true
    }

    /// Stamp via footprint on all Z planes it passes through.
    ///
    /// v0.1.8 (Roadmap 14.2): Delegates to `mark_circular_area_occupied()` which
    /// registers via pads as analytic cylinder substrate layers in the EntityGraph
    /// via `add_cylinder_substrate_layer()`. Each Z plane gets a separate substrate
    /// layer entry, making via pads visible to all spatial queries (DRC, clearance).
    /// Also generates anti-pads for non-matching copper pours.
    pub fn stamp_via(&mut self, entity_graph: &mut EntityGraph, via: &Via) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let enclosure = fabrication.min_enclosure_nm;
        let total_radius = (via.diameter_nm + 2 * enclosure) / 2;

        for z_nm in via.z_planes_between(&self.config.layer_z_positions, 0, self.bounds.depth_nm) {
            self.mark_circular_area_occupied(
                entity_graph,
                via.position,
                total_radius,
                z_nm,
                via.net_id,
            );
        }

        self.generate_antipads(entity_graph, via);
    }

    /// Generate anti-pads for vias passing through copper pours on different nets.
    ///
    /// v0.1.8 (Roadmap 14.2): For non-matching nets, calls `remove_circular_area()`
    /// which finds and removes matching Circle-shaped substrate layers from the
    /// EntityGraph and rebuilds the spatial index. For matching-net pours with
    /// thermal_relief=true, generates thermal relief spokes via the native vector
    /// `ThermalReliefGenerator` that writes directly to the EntityGraph.
    pub(super) fn generate_antipads(&mut self, entity_graph: &mut EntityGraph, via: &Via) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let clearance = fabrication.min_trace_spacing_nm;
        let antipad_radius = (via.diameter_nm + 2 * clearance) / 2;

        for z_nm in via.z_planes_between(&self.config.layer_z_positions, 0, self.bounds.depth_nm) {
            for pour in &self.copper_pours.clone() {
                if pour.z_bottom_nm == z_nm {
                    if pour.net_id != via.net_id {
                        self.remove_circular_area(entity_graph, via.position, antipad_radius, z_nm);
                    } else if let Some(hwc_parser::Expression::Variable { name, .. }) =
                        via.properties.get("thermal_relief")
                    {
                        if name == "true" {
                            use crate::geometry_router::thermal_relief::{
                                ThermalReliefConfig, ThermalReliefGenerator,
                            };
                            let generator = ThermalReliefGenerator::new(
                                ThermalReliefConfig::default(),
                                self.manufacturing_grid_nm,
                            );
                            // v0.1.8: Thermal relief generation uses EntityGraph-native
                            // vector polygons — spokes are registered as Clipper2 Path64
                            // polygons via add_polygon_substrate_layer(), not rasterized
                            // into grid cells. See thermal_relief.rs for full rationale.
                            let pad_radius = via.diameter_nm / 2;
                            let center =
                                crate::geometry::Point2D::new(via.position.0, via.position.1);
                            generator.generate_for_circular_pad(
                                center,
                                pad_radius,
                                z_nm,
                                self.routing_material_id,
                                via.net_id,
                                entity_graph,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Clear a via and remove its pads from the spatial index.
    ///
    /// v0.1.8 (Roadmap 14.2): Removes the via from the local `vias` list, then
    /// calls `remove_circular_area()` for each Z plane. This finds and removes
    /// matching Circle-shaped substrate layers from the EntityGraph and rebuilds
    /// the spatial index, ensuring via pads are no longer visible to DRC or
    /// clearance checks.
    pub fn clear_via(&mut self, entity_graph: &mut EntityGraph, via: &Via) {
        self.vias.retain(|v| {
            v.position != via.position || v.from_z_nm != via.from_z_nm || v.to_z_nm != via.to_z_nm
        });

        for z_nm in via.z_planes_between(&self.config.layer_z_positions, 0, self.bounds.depth_nm) {
            self.remove_circular_area(entity_graph, via.position, via.diameter_nm, z_nm);
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
    /// v0.1.8 (Roadmap 14.2): Uses `layer_z_positions` directly from the stackup.
    /// No legacy fallbacks — fails fast if `layer_z_positions` is empty.
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
    /// # Panics
    /// Panics if `layer_z_positions` is empty — stackup must be populated before unrolling.
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
        // v0.1.8: Fail fast — no legacy fallback. Layer Z positions must
        // be populated from the stackup before any via unrolling.
        assert!(
            !self.config.layer_z_positions.is_empty(),
            "FATAL: unroll_via_tower called with empty layer_z_positions. \
             Stackup must be parsed and layer Z positions populated before routing."
        );

        let mut via_tower = Vec::new();

        let fabrication = self
            .constraints
            .fabrication
            .as_ref()
            .expect("FATAL: Fabrication constraints required for via tower unrolling. Ensure a profile with 'via:' constraints is declared in the space definition.");

        let diameter_nm = fabrication.min_via_diameter_nm;
        let enclosure = fabrication.min_enclosure_nm;

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

                // v0.1.8: Use actual Z positions from the stackup (layer_z_positions).
                // Bounds-checked — indices are validated against layer_z_positions.len()
                // because the caller derives them from profile_layers which is parallel.
                let from_z = self.config.layer_z_positions[current_idx as usize];
                let to_z = self.config.layer_z_positions[next_idx];

                via_tower.push(Via::new_with_type(
                    ViaSpec {
                        position: pos,
                        from_z_nm: from_z,
                        to_z_nm: to_z,
                        diameter_nm,
                        net_id,
                        material_id: self.routing_material_id, // Use the active routing material context
                        enclosure_nm: enclosure,
                        board_min_z_nm: 0,
                        board_max_z_nm: self.bounds.depth_nm,
                    },
                    ViaType::Buried, // Intermediate vias in ASIC towers are buried
                ));

                current_idx = next_idx as isize;
            }
        } else {
            // PCB: emit a single through-hole via spanning the full depth
            let from_z = self.config.layer_z_positions[start_layer_idx];
            let to_z = self.config.layer_z_positions[end_layer_idx];

            via_tower.push(Via::new_with_type(
                ViaSpec {
                    position: pos,
                    from_z_nm: from_z,
                    to_z_nm: to_z,
                    diameter_nm,
                    net_id,
                    material_id: self.routing_material_id, // Use the active routing material context
                    enclosure_nm: enclosure,
                    board_min_z_nm: 0,
                    board_max_z_nm: self.bounds.depth_nm,
                },
                ViaType::ThroughHole,
            ));
        }

        via_tower
    }

    /// Generate intermediate landing pads for an ASIC via tower.
    ///
    /// Each intermediate layer in the via tower needs a small copper landing pad
    /// (annular ring) to anchor the via physically. This method stamps copper
    /// discs at each intermediate Z using the EntityGraph.
    ///
    /// v0.1.8 (Roadmap 14.2): Delegates to `mark_circular_area_occupied()` which
    /// registers landing pads as analytic cylinder substrate layers in the EntityGraph
    /// via `add_cylinder_substrate_layer()`. The first and last Z planes are skipped
    /// because they already have pads from the via itself.
    ///
    /// # Arguments
    /// * `via_tower` - The unrolled via tower from `unroll_via_tower`
    /// * `enclosures` - Per-layer enclosure sizes (layer_name → enclosure_nm)
    pub fn generate_intermediate_landing_pads(
        &mut self,
        entity_graph: &mut EntityGraph,
        via_tower: &[Via],
        enclosures: &rustc_hash::FxHashMap<String, i64>,
    ) {
        let fabrication = match &self.constraints.fabrication {
            Some(f) => f,
            None => return,
        };

        let enclosure = fabrication.min_enclosure_nm;

        for via in via_tower {
            // For each intermediate Z plane (not the first or last), stamp a landing pad
            let z_planes =
                via.z_planes_between(&self.config.layer_z_positions, 0, self.bounds.depth_nm);
            for (i, &z_nm) in z_planes.iter().enumerate() {
                if i == 0 || i == z_planes.len() - 1 {
                    continue; // Skip the start and end layers (already have pads from the via)
                }

                // Calculate landing pad radius: via_radius + enclosure
                let enclosure = enclosures.values().next().copied().unwrap_or(enclosure);
                let pad_radius = (via.diameter_nm / 2) + enclosure;

                // Stamp the landing pad
                self.mark_circular_area_occupied(
                    entity_graph,
                    via.position,
                    pad_radius,
                    z_nm,
                    via.net_id,
                );
            }
        }
    }

    /// Find the layer index for a given Z position using the profile's layer Z positions.
    ///
    /// Returns the index of the layer whose Z range contains `z_nm`.
    /// If `z_nm` is between layers, returns the index of the closest layer below.
    pub fn find_layer_index_at_z(&self, z_nm: i64) -> Option<usize> {
        if self.config.layer_z_positions.is_empty() {
            return None;
        }
        // Find the last layer whose start Z is <= z_nm
        for (i, &layer_z) in self.config.layer_z_positions.iter().enumerate().rev() {
            if z_nm >= layer_z {
                return Some(i);
            }
        }
        // z_nm is below all layers, return first layer
        Some(0)
    }

    /// Unroll a detected via into layer-by-layer vias (ASIC) or emit a single
    /// through-hole via (PCB).
    ///
    /// For ASIC (Manhattan) profiles, splits the via into individual buried vias
    /// for each adjacent layer pair it spans, with intermediate landing pads.
    /// For PCB (Octilinear) profiles, emits a single through-hole via spanning
    /// the full transition depth.
    ///
    /// v0.1.8 (Roadmap 14.2): Delegates to `unroll_via_tower()` which uses
    /// `layer_z_positions` directly from the stackup. No legacy fallbacks.
    /// If profile layer info is not available, returns the original via unchanged.
    pub fn unroll_detected_via(&self, via: &Via) -> Vec<Via> {
        if self.config.profile_layers.is_empty() || self.config.layer_z_positions.is_empty() {
            return vec![via.clone()];
        }

        let min_z = via.from_z_nm.min(via.to_z_nm);
        let max_z = via.from_z_nm.max(via.to_z_nm);

        let start_idx = self.find_layer_index_at_z(min_z).unwrap_or(0);
        let end_idx = self
            .find_layer_index_at_z(max_z)
            .unwrap_or(self.config.profile_layers.len() - 1);

        // If the via only spans one layer, no unrolling needed
        if start_idx == end_idx {
            return vec![via.clone()];
        }

        self.unroll_via_tower(
            via.position,
            start_idx,
            end_idx,
            &self.config.profile_layers,
            via.net_id,
            self.config.is_manhattan,
        )
    }
}
