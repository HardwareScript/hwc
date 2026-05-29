use super::types::*;
use crate::connectivity::{ContactMetadata, PourMetadata, SubstrateLayerMetadata};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Net binder - maps logical nets to physical islands.
pub struct NetBinder<'a> {
    pours: &'a [PourMetadata],
    contacts: &'a [ContactMetadata],
    substrate_layers: &'a [SubstrateLayerMetadata],
}

impl<'a> NetBinder<'a> {
    pub fn new(
        pours: &'a [PourMetadata],
        contacts: &'a [ContactMetadata],
        substrate_layers: &'a [SubstrateLayerMetadata],
    ) -> Self {
        Self {
            pours,
            contacts,
            substrate_layers,
        }
    }

    /// Bind logical nets to physical islands.
    ///
    /// This creates the mapping between what the code says (net names)
    /// and what the physics says (conductive islands).
    ///
    /// # Algorithm
    /// 1. For each net, find all geometry nodes labeled with that net
    /// 2. For each labeled node, find which island it belongs to
    /// 3. Group islands by net name
    ///
    /// # Returns
    /// Vector of net-to-island bindings
    pub fn bind_nets(&self, islands: &[ConductiveIsland]) -> Vec<NetIslandBinding> {
        let mut bindings: FxHashMap<CompactString, NetIslandBinding> = FxHashMap::default();

        // Build node-to-island map for fast lookup
        let mut node_to_island: FxHashMap<GeometryNodeRef, usize> = FxHashMap::default();
        for island in islands {
            for node in &island.nodes {
                node_to_island.insert(*node, island.id);
            }
        }

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Building net-to-island bindings");

        // For each pour with a net label
        for (idx, pour) in self.pours.iter().enumerate() {
            if let Some(net_name) = &pour.net {
                let node = GeometryNodeRef::Pour(idx);
                if let Some(&island_id) = node_to_island.get(&node) {
                    let binding =
                        bindings
                            .entry(net_name.clone())
                            .or_insert_with(|| NetIslandBinding {
                                net_name: net_name.clone(),
                                islands: Vec::new(),
                                expected_pins: Vec::new(),
                            });

                    if !binding.islands.contains(&island_id) {
                        binding.islands.push(island_id);
                        // println!($3"[DEBUG PHYSICAL CONTINUITY] Net '{}' -> Island {}",
                        //    net_name, island_id
                        // );
                    }
                }
            }
        }

        // For each contact with a net label
        for (idx, contact) in self.contacts.iter().enumerate() {
            if let Some(net_name) = &contact.net {
                let node = GeometryNodeRef::Contact(idx);
                if let Some(&island_id) = node_to_island.get(&node) {
                    let binding =
                        bindings
                            .entry(net_name.clone())
                            .or_insert_with(|| NetIslandBinding {
                                net_name: net_name.clone(),
                                islands: Vec::new(),
                                expected_pins: Vec::new(),
                            });

                    if !binding.islands.contains(&island_id) {
                        binding.islands.push(island_id);
                        // println!($3"[DEBUG PHYSICAL CONTINUITY] Net '{}' -> Island {}",
                        //   net_name, island_id
                        //   );
                    }
                }
            }
        }

        // For each substrate layer with a net label
        for (idx, layer) in self.substrate_layers.iter().enumerate() {
            if let Some(net_name) = &layer.net_name {
                let node = GeometryNodeRef::SubstrateLayer(idx);
                if let Some(&island_id) = node_to_island.get(&node) {
                    let binding =
                        bindings
                            .entry(net_name.clone())
                            .or_insert_with(|| NetIslandBinding {
                                net_name: net_name.clone(),
                                islands: Vec::new(),
                                expected_pins: Vec::new(),
                            });

                    if !binding.islands.contains(&island_id) {
                        binding.islands.push(island_id);
                        // println!($3"[DEBUG PHYSICAL CONTINUITY] Net '{}' -> Island {}",
                        //      net_name, island_id
                        // );
                    }
                }
            }
        }

        bindings.into_values().collect()
    }
}
