use super::types::*;
use crate::connectivity::SubstrateLayerMetadata;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

pub struct NetBinder;

impl NetBinder {
    pub fn new() -> Self {
        Self
    }

    pub fn bind_nets(&self, _islands: &[ConductiveIsland]) -> Vec<NetIslandBinding> {
        Vec::new()
    }

    pub fn bind_nets_from_substrates(
        substrate_layers: &[SubstrateLayerMetadata],
        route_segments: &[RouteSegmentMetadata],
        islands: &[ConductiveIsland],
    ) -> Vec<NetIslandBinding> {
        let mut bindings: FxHashMap<CompactString, NetIslandBinding> = FxHashMap::default();

        let mut node_to_island: FxHashMap<GeometryNodeRef, usize> = FxHashMap::default();
        for island in islands {
            for node in &island.nodes {
                node_to_island.insert(*node, island.id);
            }
        }

        for (idx, layer) in substrate_layers.iter().enumerate() {
            if let Some(net_name) = &layer.net_name {
                let node = GeometryNodeRef::SubstrateLayer(idx);
                if let Some(&island_id) = node_to_island.get(&node) {
                    let binding = bindings.entry(net_name.clone()).or_insert_with(|| NetIslandBinding {
                        net_name: net_name.clone(),
                        islands: Vec::new(),
                        expected_pins: Vec::new(),
                    });
                    if !binding.islands.contains(&island_id) {
                        binding.islands.push(island_id);
                    }
                }
            }
        }

        for (idx, seg) in route_segments.iter().enumerate() {
            if let Some(net_name) = &seg.net_name {
                let node = GeometryNodeRef::RouteSegment(idx);
                if let Some(&island_id) = node_to_island.get(&node) {
                    let binding = bindings.entry(net_name.clone()).or_insert_with(|| NetIslandBinding {
                        net_name: net_name.clone(),
                        islands: Vec::new(),
                        expected_pins: Vec::new(),
                    });
                    if !binding.islands.contains(&island_id) {
                        binding.islands.push(island_id);
                    }
                }
            }
        }

        bindings.into_values().collect()
    }
}
