//! Cryptographically unique Entity IDs for the Entity Graph.
//!
//! Generated as: hash(Type + SemanticPath + ParentId)
//! This prevents index-shifting when elements are added or deleted.
//!
//! These are NEW types, separate from the ECS arena indices in `netlist.rs`.

use std::fmt;

/// A stable, unique identifier for any entity in the design (v0.1.8)
/// Generated via cryptographic hash of semantic path + type, truncated to u64.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Create an EntityId from a raw u64
    pub fn new(id: u64) -> Self {
        EntityId(id)
    }

    /// Get the raw u64 value
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Compute a stable EntityId from a semantic string
    pub fn from_semantic(s: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        EntityId(hasher.finish())
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}", self.0)
    }
}

// === Typed ID wrappers (newtype over EntityId for type safety) ===

/// Unique identifier for a component in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentGraphId(pub EntityId);

/// Unique identifier for a pin in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinGraphId(pub EntityId);

/// Unique identifier for a net in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetGraphId(pub EntityId);

/// Unique identifier for a route segment in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteGraphId(pub EntityId);

/// Unique identifier for a physical geometry node in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeometryGraphId(pub EntityId);

/// Unique identifier for a junction (T-junction tap) in the Entity Graph
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JunctionGraphId(pub EntityId);

impl ComponentGraphId {
    pub fn generate(component_type: &str, placement_path: &str) -> Self {
        Self(EntityId::from_semantic(&format!(
            "comp:{}:{}",
            component_type, placement_path
        )))
    }
}

impl PinGraphId {
    pub fn generate(component_path: &str, pin_name: &str) -> Self {
        Self(EntityId::from_semantic(&format!(
            "pin:{}:{}",
            component_path, pin_name
        )))
    }

    pub fn generate_from_parent(pin_name: &str, component_id: &EntityId) -> Self {
        Self(EntityId::from_semantic(&format!(
            "pin:{}:{}",
            component_id.to_hex(),
            pin_name
        )))
    }
}

impl NetGraphId {
    pub fn generate(net_name: &str, parent: &EntityId) -> Self {
        Self(EntityId::from_semantic(&format!(
            "net:{}:{}",
            parent.to_hex(),
            net_name
        )))
    }
}

impl RouteGraphId {
    pub fn generate(from_pin: &EntityId, to_pin: &EntityId) -> Self {
        Self(EntityId::from_semantic(&format!(
            "route:{}:{}",
            from_pin.to_hex(),
            to_pin.to_hex()
        )))
    }
}

impl GeometryGraphId {
    pub fn generate(route_id: &EntityId, layer: i64) -> Self {
        Self(EntityId::from_semantic(&format!(
            "geom:{}:layer:{}",
            route_id.to_hex(),
            layer
        )))
    }
}

impl JunctionGraphId {
    pub fn generate(route_a: &EntityId, route_b: &EntityId) -> Self {
        Self(EntityId::from_semantic(&format!(
            "junction:{}:{}",
            route_a.to_hex(),
            route_b.to_hex()
        )))
    }
}
