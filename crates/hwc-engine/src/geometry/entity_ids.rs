//! Cryptographically unique Entity IDs for the Entity Graph.
//!
//! Generated as: hash(Type + SemanticPath + ParentId)
//! This prevents index-shifting when elements are added or deleted.
//!
//! These are NEW types, separate from the ECS arena indices in `netlist.rs`.

use std::fmt;

use sha2::{Digest, Sha256};

/// A cryptographically unique identifier for an Entity Graph node.
/// Generated as: hash(Type + SemanticPath + ParentId)
/// This prevents index-shifting when elements are added or deleted.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId([u8; 32]);

impl EntityId {
    /// Create an EntityId by hashing type + semantic path + parent
    pub fn generate(type_tag: &str, semantic_path: &str, parent_id: &EntityId) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(type_tag.as_bytes());
        hasher.update(b"\0");
        hasher.update(semantic_path.as_bytes());
        hasher.update(b"\0");
        hasher.update(parent_id.0);
        let hash = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&hash);
        Self(bytes)
    }

    /// Create a root entity (no parent)
    pub fn root(type_tag: &str, semantic_path: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(type_tag.as_bytes());
        hasher.update(b"\0");
        hasher.update(semantic_path.as_bytes());
        hasher.update(b"\0");
        let hash = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&hash);
        Self(bytes)
    }

    /// Get raw bytes
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Format as hex string for display
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.to_hex();
        write!(f, "EntityId({}…)", &hex[..8])
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.to_hex();
        write!(f, "{}", &hex[..16])
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
    pub fn generate(component_type: &str, placement_path: &str, parent: &EntityId) -> Self {
        Self(EntityId::generate(
            "Component",
            &format!("{component_type}:{placement_path}"),
            parent,
        ))
    }
}

impl PinGraphId {
    pub fn generate(pin_name: &str, component_id: &EntityId) -> Self {
        Self(EntityId::generate("Pin", pin_name, component_id))
    }
}

impl NetGraphId {
    pub fn generate(net_name: &str, parent: &EntityId) -> Self {
        Self(EntityId::generate("Net", net_name, parent))
    }
}

impl RouteGraphId {
    pub fn generate(from_pin: &EntityId, to_pin: &EntityId) -> Self {
        Self(EntityId::generate(
            "Route",
            &format!("{}:{}", from_pin.to_hex(), to_pin.to_hex()),
            &EntityId::root("RouteSegment", "global"),
        ))
    }
}

impl GeometryGraphId {
    pub fn generate(route_id: &EntityId, layer: i64) -> Self {
        Self(EntityId::generate(
            "Geometry",
            &format!("layer:{layer}"),
            route_id,
        ))
    }
}

impl JunctionGraphId {
    pub fn generate(route_a: &EntityId, route_b: &EntityId) -> Self {
        Self(EntityId::generate(
            "Junction",
            &format!("{}:{}", route_a.to_hex(), route_b.to_hex()),
            &EntityId::root("Junction", "global"),
        ))
    }
}
