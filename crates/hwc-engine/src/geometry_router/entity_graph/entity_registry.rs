//! Entity registration and lookup methods for EntityGraph.

use crate::geometry::entity_ids::*;
use crate::geometry::BoundingBox;
use crate::netlist::NetId;

use super::{EntityData, EntityGraph, EntityType};

impl EntityGraph {
    /// Register a component pin and return its EntityId (v0.1.8)
    pub fn register_component_pin(
        &mut self,
        component_name: &str,
        pin_name: &str,
        bbox: BoundingBox,
        net_id: Option<NetId>,
    ) -> EntityId {
        let id = EntityId::from_semantic(&format!("pin:{}:{}", component_name, pin_name));
        eprintln!(
            "[DEBUG register_component_pin] Registering '{}.{}' with EntityId: {}, net_id: {:?}",
            component_name, pin_name, id, net_id
        );
        self.entity_registry.insert(
            id,
            EntityData {
                entity_type: EntityType::ComponentPin,
                bbox,
                net_id,
                name: format!("{}.{}", component_name, pin_name).into(),
                layer_z: None,
            },
        );
        id
    }

    /// Register a space-level pour/pad and return its EntityId (v0.1.8)
    pub fn register_space_entity(
        &mut self,
        name: &str,
        bbox: BoundingBox,
        net_id: Option<NetId>,
        layer_z: i64,
    ) -> EntityId {
        let id = EntityId::from_semantic(&format!("space:{}", name));

        self.entity_registry.insert(
            id,
            EntityData {
                entity_type: EntityType::SpacePour,
                bbox,
                net_id,
                name: name.into(),
                layer_z: Some(layer_z),
            },
        );
        id
    }

    /// Get bounding box for a space entity by name (v0.1.9.1)
    pub fn get_space_entity_bbox(&self, name: &str) -> Option<BoundingBox> {
        let entity_id = EntityId::from_semantic(&format!("space:{}", name));
        self.entity_registry
            .get(&entity_id)
            .map(|entity_data| entity_data.bbox)
    }

    /// Get the bounding box for a component pin (v0.1.9.1)
    pub fn get_component_pin_bbox(
        &self,
        component_name: &str,
        pin_name: &str,
    ) -> Option<BoundingBox> {
        let entity_id = EntityId::from_semantic(&format!("pin:{}:{}", component_name, pin_name));
        self.entity_registry
            .get(&entity_id)
            .map(|entity_data| entity_data.bbox)
    }

    /// Update net assignment for an entity (v0.1.8)
    pub fn set_entity_net(&mut self, entity_name: &str, net_name: &str) {
        let net_id = self.netlist.get_or_create_net(net_name);
        for data in self.entity_registry.values_mut() {
            if data.name == entity_name {
                data.net_id = Some(net_id);
            }
        }
    }

    /// Lookup entity data by EntityId (v0.1.8)
    pub fn get_entity_data(&self, id: EntityId) -> Result<&EntityData, String> {
        let result = self.entity_registry.get(&id);
        if result.is_none() {
            eprintln!(
                "[DEBUG get_entity_data] EntityId {} NOT FOUND in registry (size: {})",
                id,
                self.entity_registry.len()
            );
        }
        result.ok_or_else(|| format!("EntityId {} not found in registry", id))
    }

    /// Get all registered entity IDs (v0.1.8)
    pub fn iter_entity_ids(&self) -> impl Iterator<Item = &EntityId> {
        self.entity_registry.keys()
    }

    /// Iterate over all entity registry entries (v0.2.1)
    /// Used for hierarchical space flattening
    pub fn iter_entity_registry(&self) -> impl Iterator<Item = (&EntityId, &EntityData)> {
        self.entity_registry.iter()
    }

    /// Iterate over all (entity_name, PhysicalInterface) entries in the interface map (v0.2.1).
    ///
    /// Used by the hierarchical flattener to clone and re-register interfaces
    /// under new hierarchical names in the parent EntityGraph.
    pub fn iter_entity_interfaces(
        &self,
    ) -> impl Iterator<
        Item = (
            &compact_str::CompactString,
            &crate::geometry_router::connection_interface::PhysicalInterface,
        ),
    > {
        self.entity_interface_map
            .iter()
            .filter_map(|(name, id)| self.interface_database.get(id).map(|iface| (name, iface)))
    }

    /// Register an entity from existing EntityData (v0.2.1)
    /// Used for hierarchical space flattening with transformed entities
    pub fn register_entity_from_data(
        &mut self,
        entity_id: EntityId,
        entity_data: EntityData,
    ) -> Result<(), String> {
        if self.entity_registry.contains_key(&entity_id) {
            return Err(format!(
                "Entity {:?} already registered in entity graph",
                entity_id
            ));
        }
        self.entity_registry.insert(entity_id, entity_data);
        Ok(())
    }

    /// Query: Which component name is at this point?
    pub fn point_in_component(&self, x: i64, y: i64, z: i64) -> Option<compact_str::CompactString> {
        for meta in &self.component_metadata {
            if meta.bbox.contains(crate::geometry::Point3D::new(x, y, z)) {
                return Some(meta.name.clone());
            }
        }
        None
    }

    /// Query: Get the bounding box of a pour at this position.
    pub fn get_pour_bbox_at_position(&self, x: i64, y: i64, z: i64) -> Option<BoundingBox> {
        for layer in &self.substrate_layers {
            if layer.bbox.contains(crate::geometry::Point3D::new(x, y, z)) {
                return Some(layer.bbox);
            }
        }
        None
    }
}
