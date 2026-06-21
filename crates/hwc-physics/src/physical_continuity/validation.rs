use super::types::*;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

pub struct ContinuityValidator;

impl ContinuityValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(
        &self,
        islands: &[ConductiveIsland],
        bindings: &[NetIslandBinding],
        enable_p43: bool,
    ) -> Vec<PhysicalContinuityViolation> {
        let mut violations = Vec::new();

        self.check_disconnected_nets(bindings, islands, &mut violations);

        let island_to_nets = self.build_island_to_nets_map(bindings);
        self.check_short_circuits(&island_to_nets, islands, &mut violations);

        if enable_p43 {
            self.check_floating_conductors(&island_to_nets, islands, &mut violations);
        }

        violations
    }

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

                let suggested_fix = self.suggest_bridge_fix(&binding.net_name, islands, &binding.islands);

                violations.push(PhysicalContinuityViolation::DisconnectedNet {
                    net_name: binding.net_name.clone(),
                    island_count: binding.islands.len(),
                    islands: island_summaries,
                    suggested_fix,
                });
            }
        }
    }

    fn check_short_circuits(
        &self,
        island_to_nets: &FxHashMap<usize, Vec<CompactString>>,
        islands: &[ConductiveIsland],
        violations: &mut Vec<PhysicalContinuityViolation>,
    ) {
        for (island_id, net_names) in island_to_nets.iter() {
            if net_names.len() > 1 {
                let island = &islands[*island_id];
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

    fn check_floating_conductors(
        &self,
        island_to_nets: &FxHashMap<usize, Vec<CompactString>>,
        islands: &[ConductiveIsland],
        violations: &mut Vec<PhysicalContinuityViolation>,
    ) {
        for island in islands {
            let has_net_assignment = island_to_nets.contains_key(&island.id);

            if has_net_assignment && island.pins.is_empty() {
                let material_name = match island.material {
                    2 => "Copper",
                    _ => "Unknown",
                };

                violations.push(PhysicalContinuityViolation::FloatingConductor {
                    island_id: island.id,
                    material_name: material_name.to_string().into(),
                    bbox: island.bbox.clone(),
                    suggested_fix: format!(
                        "Island {} has conductive geometry but no component pins touch it. \
                         This conductor is electrically floating and may cause:\n    \
                         EMI antenna effects, signal integrity issues, unpredictable coupling.\n    \
                         Suggested fix: Either connect a component pin to this geometry, \
                         or remove it if it's unintentional.",
                        island.id
                    )
                    .into(),
                });
            }
        }
    }

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

    fn suggest_bridge_fix(
        &self,
        net_name: &str,
        islands: &[ConductiveIsland],
        island_ids: &[usize],
    ) -> CompactString {
        if island_ids.len() < 2 {
            return "Unknown gap type.".into();
        }

        let island_a = &islands[island_ids[0]];
        let island_b = &islands[island_ids[1]];

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

        let z_gap = if island_a.bbox.max_z < island_b.bbox.min_z {
            island_b.bbox.min_z - island_a.bbox.max_z
        } else if island_b.bbox.max_z < island_a.bbox.min_z {
            island_a.bbox.min_z - island_b.bbox.max_z
        } else {
            0
        };

        if z_gap > 0 {
            return format!(
                "Z-layer gap detected: {} nm between islands {} and {}.\n    \
                 Island {} is at z:{}-{}, Island {} is at z:{}-{}.\n    \
                 Suggested fix: Add a contact (via) to bridge the gap on net '{}'.",
                z_gap,
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

        format!(
            "Islands {} and {} on net '{}' are not physically connected.\n    \
             Suggested fix: Add geometry (pour, contact, or route) to bridge the gap.",
            island_a.id, island_b.id, net_name
        )
        .into()
    }
}
