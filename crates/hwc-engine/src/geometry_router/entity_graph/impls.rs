//! Trait implementations for EntityGraph.

use crate::geometry_router::scene_graph::SceneGraph;
use crate::geometry_router::spatial_index::DynamicSpatialIndex;
use crate::netlist::NetlistArena;

use super::EntityGraph;

impl Default for EntityGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EntityGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityGraph")
            .field("substrate_layers", &self.substrate_layers.len())
            .field("component_metadata", &self.component_metadata.len())
            .field("component_pins", &self.component_pins.len())
            .field("routed_segments", &self.routed_segments.len())
            .finish()
    }
}

impl Clone for EntityGraph {
    fn clone(&self) -> Self {
        Self {
            netlist: NetlistArena::new(),
            scene: SceneGraph::new(),
            spatial: DynamicSpatialIndex::new(),
            entity_registry: self.entity_registry.clone(),
            component_id_map: self.component_id_map.clone(),
            net_id_map: self.net_id_map.clone(),
            _next_stamp_id: self._next_stamp_id,
            _next_instance_id: self._next_instance_id,
            substrate_layers: self.substrate_layers.clone(),
            component_metadata: self.component_metadata.clone(),
            component_pins: self.component_pins.clone(),
            routed_segments: self.routed_segments.clone(),
            interface_database: self.interface_database.clone(),
            entity_interface_map: self.entity_interface_map.clone(),
            next_interface_id: self.next_interface_id,
            component_interfaces: self.component_interfaces.clone(),
            pin_interface_map: self.pin_interface_map.clone(),
        }
    }
}
