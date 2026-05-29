use super::types::*;
use crate::connectivity::{BoundingBox, ContactMetadata, PourMetadata, SubstrateLayerMetadata};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Continuity validator - checks for P41, P42, and P43 violations.
pub struct ContinuityValidator<'a> {
    voxel_size_z_nm: i64,
    _pours: &'a [PourMetadata],
    contacts: &'a [ContactMetadata],
    _substrate_layers: &'a [SubstrateLayerMetadata],
}

impl<'a> ContinuityValidator<'a> {
    pub fn new(
        voxel_size_z_nm: i64,
        pours: &'a [PourMetadata],
        contacts: &'a [ContactMetadata],
        substrate_layers: &'a [SubstrateLayerMetadata],
    ) -> Self {
        Self {
            voxel_size_z_nm,
            _pours: pours,
            contacts,
            _substrate_layers: substrate_layers,
        }
    }

    /// Validate physical continuity and detect violations.
    ///
    /// This is the final validation step that checks:
    /// 1. Each net has exactly 1 island (no disconnections) - P41
    /// 2. Each island has exactly 1 net (no shorts) - P42
    /// 3. Each island has at least 1 pin (no floating conductors) - P43
    ///
    /// # Arguments
    /// * `islands` - All conductive islands built by flood-fill
    /// * `bindings` - Net-to-island bindings
    /// * `enable_p43` - Whether to check for floating conductors (requires pin detection)
    ///
    /// # Returns
    /// Vector of physical continuity violations
    pub fn validate(
        &self,
        islands: &[ConductiveIsland],
        bindings: &[NetIslandBinding],
        enable_p43: bool,
    ) -> Vec<PhysicalContinuityViolation> {
        let mut violations = Vec::new();

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Validating continuity for {} nets",
        //     bindings.len()
        // );

        // Check 1: Each net should have exactly 1 island (P41)
        self.check_disconnected_nets(bindings, islands, &mut violations);

        // Check 2: Each island should have exactly 1 net (P42)
        let island_to_nets = self.build_island_to_nets_map(bindings);
        self.check_short_circuits(&island_to_nets, islands, &mut violations);

        // Check 3: Each island should have at least 1 pin (P43)
        if enable_p43 {
            self.check_floating_conductors(&island_to_nets, islands, &mut violations);
        } else {
            // println!($3"[DEBUG PHYSICAL CONTINUITY] P43 check skipped (pin detection not enabled)");
        }

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Found {} violations",
        //     violations.len()
        // );

        violations
    }

    /// Check for disconnected nets (P41).
    fn check_disconnected_nets(
        &self,
        bindings: &[NetIslandBinding],
        islands: &[ConductiveIsland],
        violations: &mut Vec<PhysicalContinuityViolation>,
    ) {
        for binding in bindings {
            if binding.islands.len() > 1 {
                let island_summaries: Vec<IslandSummary> = binding
                    .islands
                    .iter()
                    .map(|&id| {
                        let island = &islands[id];
                        IslandSummary {
                            id: island.id,
                            bbox: island.bbox.clone(),
                            pin_count: island.pins.len(),
                            node_count: island.nodes.len(),
                        }
                    })
                    .collect();

                let suggested_fix =
                    self.suggest_bridge_fix(&binding.net_name, islands, &binding.islands);

                // println!($3"[DEBUG PHYSICAL CONTINUITY] VIOLATION: Net '{}' has {} disconnected islands",
                //    binding.net_name,
                //     binding.islands.len()
                //  );

                violations.push(PhysicalContinuityViolation::DisconnectedNet {
                    net_name: binding.net_name.clone(),
                    island_count: binding.islands.len(),
                    islands: island_summaries,
                    suggested_fix,
                });
            }
        }
    }

    /// Check for short circuits (P42).
    fn check_short_circuits(
        &self,
        island_to_nets: &FxHashMap<usize, Vec<CompactString>>,
        islands: &[ConductiveIsland],
        violations: &mut Vec<PhysicalContinuityViolation>,
    ) {
        for (island_id, net_names) in island_to_nets.iter() {
            if net_names.len() > 1 {
                let island = &islands[*island_id];

                // println!($3"[DEBUG PHYSICAL CONTINUITY] VIOLATION: Island {} has {} nets: {:?}",
                //   island_id,
                //    net_names.len(),
                //    net_names
                // );

                violations.push(PhysicalContinuityViolation::ShortCircuit {
                    island_id: *island_id,
                    net_names: net_names.clone(),
                    overlap_location: format!(
                        "x:{}-{}, y:{}-{}, z:{}-{}",
                        island.bbox.min_x / 1_000_000,
                        island.bbox.max_x / 1_000_000,
                        island.bbox.min_y / 1_000_000,
                        island.bbox.max_y / 1_000_000,
                        island.bbox.min_z / 1_000_000,
                        island.bbox.max_z / 1_000_000
                    ).into(),
                    suggested_fix:
                        "Separate the overlapping geometry or verify that these nets should be connected."
                            .to_string().into(),
                });
            }
        }
    }

    /// Check for floating conductors (P43).
    fn check_floating_conductors(
        &self,
        island_to_nets: &FxHashMap<usize, Vec<CompactString>>,
        islands: &[ConductiveIsland],
        violations: &mut Vec<PhysicalContinuityViolation>,
    ) {
        // println!($3"[DEBUG PHYSICAL CONTINUITY] Running P43 check (Floating Conductor Detection)");

        for island in islands {
            // Skip islands that have no net assignment (these are caught by the pre-check)
            // We only check islands that ARE assigned to a net but have no pins
            let has_net_assignment = island_to_nets.contains_key(&island.id);

            if has_net_assignment && island.pins.is_empty() {
                // Get the material name for better error reporting
                let material_name = match island.material {
                    2 => "Copper",
                    _ => "Unknown",
                };

                // println!($3"[DEBUG PHYSICAL CONTINUITY] VIOLATION: Island {} has no pins (P43)",
                // island.id
                //  );

                violations.push(PhysicalContinuityViolation::FloatingConductor {
                    island_id: island.id,
                    material_name: material_name.to_string().into(),
                    bbox: island.bbox.clone(),
                    suggested_fix: format!(
                        "Island {} has conductive geometry but no component pins touch it. \
                         This conductor is electrically floating and may cause:\n    \
                         • EMI antenna effects\n    \
                         • Signal integrity issues\n    \
                         • Unpredictable coupling\n    \
                         Suggested fix: Either connect a component pin to this geometry, \
                         or remove it if it's unintentional.",
                        island.id
                    )
                    .into(),
                });
            }
        }
    }

    /// Build a map from island ID to net names.
    fn build_island_to_nets_map(
        &self,
        bindings: &[NetIslandBinding],
    ) -> FxHashMap<usize, Vec<CompactString>> {
        let mut island_to_nets: FxHashMap<usize, Vec<CompactString>> = FxHashMap::default();
        for binding in bindings {
            for &island_id in &binding.islands {
                island_to_nets
                    .entry(island_id)
                    .or_default()
                    .push(binding.net_name.clone());
            }
        }
        island_to_nets
    }

    /// Suggest a fix for disconnected islands.
    ///
    /// This analyzes the gap between islands and suggests:
    /// - Adding a via if there's a Z-gap
    /// - Adding a trace if there's an XY-gap
    /// - Checking for unassigned geometry that might bridge the gap
    fn suggest_bridge_fix(
        &self,
        net_name: &str,
        islands: &[ConductiveIsland],
        island_ids: &[usize],
    ) -> CompactString {
        if island_ids.len() < 2 {
            return "Unknown gap type.".into();
        }

        // Analyze the gap between the first two islands
        let island_a = &islands[island_ids[0]];
        let island_b = &islands[island_ids[1]];

        // Check for Z-gap
        let z_gap = if island_a.bbox.max_z < island_b.bbox.min_z {
            island_b.bbox.min_z - island_a.bbox.max_z
        } else if island_b.bbox.max_z < island_a.bbox.min_z {
            island_a.bbox.min_z - island_b.bbox.max_z
        } else {
            0
        };

        if z_gap > self.voxel_size_z_nm {
            let gap_layers = z_gap / self.voxel_size_z_nm;
            return format!(
                "Z-layer gap detected: {} nm ({} layers) between islands {} and {}.\n    \
                 Island {} is at z:{}-{}, Island {} is at z:{}-{}.\n    \
                 Suggested fix: Add a contact (via) to bridge the gap on net '{}'.",
                z_gap,
                gap_layers,
                island_a.id,
                island_b.id,
                island_a.id,
                island_a.bbox.min_z / 1_000_000,
                island_a.bbox.max_z / 1_000_000,
                island_b.id,
                island_b.bbox.min_z / 1_000_000,
                island_b.bbox.max_z / 1_000_000,
                net_name
            )
            .into();
        }

        // Check for XY-gap
        let x_gap = if island_a.bbox.max_x < island_b.bbox.min_x {
            island_b.bbox.min_x - island_a.bbox.max_x
        } else if island_b.bbox.max_x < island_a.bbox.min_x {
            island_a.bbox.min_x - island_b.bbox.max_x
        } else {
            0
        };

        let y_gap = if island_a.bbox.max_y < island_b.bbox.min_y {
            island_b.bbox.min_y - island_a.bbox.max_y
        } else if island_b.bbox.max_y < island_a.bbox.min_y {
            island_a.bbox.min_y - island_b.bbox.max_y
        } else {
            0
        };

        if x_gap > 0 || y_gap > 0 {
            return format!(
                "XY-plane gap detected between islands {} and {}.\n    \
                 X-gap: {} mm, Y-gap: {} mm.\n    \
                 Suggested fix: Add a pour or route to bridge the gap on net '{}'.",
                island_a.id,
                island_b.id,
                x_gap / 1_000_000,
                y_gap / 1_000_000,
                net_name
            )
            .into();
        }

        // Check for unassigned geometry that might bridge the gap
        for contact in self.contacts.iter() {
            if contact.net.is_some() {
                continue; // Already assigned
            }

            if let Some(bbox) = &contact.bbox {
                // Does this contact touch both islands?
                if self.nodes_touch(bbox, &island_a.bbox) && self.nodes_touch(bbox, &island_b.bbox)
                {
                    return format!(
                        "Contact '{}' physically bridges islands {} and {} but has no 'net:' assignment.\n    \
                         Suggested fix: Add 'net: {}' to contact '{}'.",
                        contact.name, island_a.id, island_b.id, net_name, contact.name
                    ).into();
                }
            }
        }

        format!(
            "Islands {} and {} on net '{}' are not physically connected.\n    \
             Suggested fix: Add geometry (pour, contact, or route) to bridge the gap.",
            island_a.id, island_b.id, net_name
        )
        .into()
    }

    /// Check if two bounding boxes physically touch.
    fn nodes_touch(&self, a: &BoundingBox, b: &BoundingBox) -> bool {
        let x_overlap = a.min_x < b.max_x && a.max_x > b.min_x;
        let y_overlap = a.min_y < b.max_y && a.max_y > b.min_y;

        if !x_overlap || !y_overlap {
            return false;
        }

        let z_volume_overlap = a.min_z < b.max_z && a.max_z > b.min_z;
        let z_face_contact = (a.max_z == b.min_z) || (b.max_z == a.min_z);

        z_volume_overlap || z_face_contact
    }
}
