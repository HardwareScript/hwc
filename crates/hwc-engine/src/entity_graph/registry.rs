//! Bi-directional Identity Registry (Phase 1)
//!
//! Provides O(1) bi-directional lookup between 64-bit `EntityId` and
//! canonical hierarchical path strings.

use super::identity::{EntityId, HierarchicalPath};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityRegistry {
    id_to_path: FxHashMap<EntityId, CompactString>,
    path_to_id: FxHashMap<CompactString, EntityId>,
}

impl IdentityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a hierarchical path and its corresponding EntityId.
    pub fn register(&mut self, id: EntityId, path: &HierarchicalPath) {
        let path_str = path.to_canonical_string();
        self.id_to_path.insert(id, path_str.clone());
        self.path_to_id.insert(path_str, id);
    }

    /// Registers from an explicit canonical path string.
    pub fn register_str(&mut self, id: EntityId, canonical_path: &str) {
        let path_str = CompactString::new(canonical_path);
        self.id_to_path.insert(id, path_str.clone());
        self.path_to_id.insert(path_str, id);
    }

    /// O(1) lookup: EntityId -> canonical path string.
    pub fn get_path(&self, id: EntityId) -> Option<&CompactString> {
        self.id_to_path.get(&id)
    }

    /// O(1) lookup: canonical path string -> EntityId.
    pub fn get_id(&self, canonical_path: &str) -> Option<EntityId> {
        self.path_to_id.get(canonical_path).copied()
    }

    /// Returns the number of registered entities.
    pub fn len(&self) -> usize {
        self.id_to_path.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.id_to_path.is_empty()
    }
}
