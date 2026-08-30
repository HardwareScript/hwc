//! Base Silicon Snapshot Lock for Freeze-Silicon ECO (Phase 1)
//!
//! Immutable cryptographic snapshot artifact produced upon tapeout, guaranteeing
//! that Base Silicon Layers 1-20 remain 100% untouched during ECO edits.

use super::identity::EntityId;
use compact_str::CompactString;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

/// Immutable cryptographic snapshot of base silicon layers (Layers 1-20).
/// Produced when a design is taped out; ingested during Freeze-Silicon ECO mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseSiliconLock {
    /// 128-bit checksum of all base diffusion, poly, and well geometries
    pub base_checksum: u128,
    /// Set of all pre-fabricated base entity IDs that cannot be mutated or moved
    pub frozen_entity_ids: FxHashSet<EntityId>,
    /// Available uncommitted Gate-Array filler cells registered on the wafer
    pub spare_ga_filler_ids: Vec<EntityId>,
    /// Base layer names that are strictly immutable
    pub locked_layers: Vec<CompactString>,
}

impl Default for BaseSiliconLock {
    fn default() -> Self {
        Self {
            base_checksum: 0,
            frozen_entity_ids: FxHashSet::default(),
            spare_ga_filler_ids: Vec::new(),
            locked_layers: vec![
                CompactString::new("diff"),
                CompactString::new("poly"),
                CompactString::new("licon"),
                CompactString::new("psdm"),
                CompactString::new("nsdm"),
                CompactString::new("nwell"),
                CompactString::new("pwell"),
                CompactString::new("tap"),
                CompactString::new("rpm"),
                CompactString::new("npc"),
            ],
        }
    }
}

impl BaseSiliconLock {
    pub fn new(
        base_checksum: u128,
        frozen_entity_ids: FxHashSet<EntityId>,
        spare_ga_filler_ids: Vec<EntityId>,
        locked_layers: Vec<CompactString>,
    ) -> Self {
        Self {
            base_checksum,
            frozen_entity_ids,
            spare_ga_filler_ids,
            locked_layers,
        }
    }

    /// Checks if a specific entity is frozen on the base wafer.
    pub fn is_entity_locked(&self, id: EntityId) -> bool {
        self.frozen_entity_ids.contains(&id)
    }

    /// Checks if a named physical layer is locked against mutations.
    pub fn is_layer_locked(&self, layer_name: &str) -> bool {
        self.locked_layers.iter().any(|l| l.as_str() == layer_name)
    }
}
