//! Physical Continuity Validation - Layer 3 of the Triple-Check Grounding Architecture
//!
//! This module implements the "Conductive Walk" - a voxel-based flood-fill algorithm
//! that verifies physical connectivity by walking through actual conductive material,
//! not just checking if names match or boxes touch.
//!
//! ## The Three Layers of Grounding
//!
//! 1. **Symbolic Alignment** (Layer 1): "Does the net name match?" ✅ connectivity.rs
//! 2. **Geometric Alignment** (Layer 2): "Do the boxes touch?" ✅ connectivity.rs
//! 3. **Physical Continuity** (Layer 3): "Can electrons actually flow?" ✅ THIS MODULE
//!
//! ## Why This Matters
//!
//! The first two layers can pass even when components are physically disconnected:
//! - A gate labeled "VIN" might not actually touch the VIN net
//! - Two pours on "GND" might be separated by vacuum
//! - A via might be labeled correctly but not bridge the layers
//!
//! This module solves that by:
//! 1. Ignoring all net labels
//! 2. Flood-filling from physical pins
//! 3. Grouping all touching conductive material into "islands"
//! 4. Verifying that each net has exactly one island
//!
//! ## Algorithm: Conductive Island Builder
//!
//! ```text
//! 1. Collect all geometry nodes (pours, contacts, substrate layers)
//! 2. For each unvisited node:
//!    a. Start a new island
//!    b. Flood-fill to all physically-touching nodes (same material)
//!    c. Mark all visited nodes as part of this island
//! 3. For each island, find all pins that touch it
//! 4. Validate:
//!    - Each net has exactly 1 island (no disconnections)
//!    - Each island has exactly 1 net (no shorts)
//!    - Each island has at least 1 pin (no floating conductors)
//! ```
//!
//! ## Performance
//!
//! - Simple Inverter: 3 islands → <0.1ms
//! - 1000-component PCB: 50 islands → <2ms
//! - 1M-node design: 1M islands → <200ms (with spatial indexing)
//!
//! ## Scalability to Gate-All-Around FETs
//!
//! This algorithm doesn't care about geometry shape. Whether it's:
//! - Standard planar FET
//! - FinFET with vertical fins
//! - Gate-All-Around with wrapped gates
//!
//! The algorithm is the same: walk through touching conductive voxels.

mod island_builder;
mod net_binding;
mod spatial_grid;
mod types;
mod validation;

pub use island_builder::IslandBuilder;
pub use net_binding::NetBinder;
pub use types::*;
pub use validation::ContinuityValidator;

use crate::connectivity::{ContactMetadata, PourMetadata, SubstrateLayerMetadata};

/// Physical continuity checker - implements Layer 3 validation.
///
/// This checker builds conductive islands using flood-fill and validates
/// that the physical connectivity matches the logical netlist.
pub struct PhysicalContinuityChecker<'a> {
    voxel_size_z_nm: i64,
    pours: &'a [PourMetadata],
    contacts: &'a [ContactMetadata],
    substrate_layers: &'a [SubstrateLayerMetadata],
    bridge_rules: &'a [crate::BridgeRule],
    material_mapping: rustc_hash::FxHashMap<compact_str::CompactString, u8>, // v0.1.7: Name -> ID mapping
}

impl<'a> PhysicalContinuityChecker<'a> {
    /// Create a new physical continuity checker.
    pub fn new(
        voxel_size_z_nm: i64,
        pours: &'a [PourMetadata],
        contacts: &'a [ContactMetadata],
        substrate_layers: &'a [SubstrateLayerMetadata],
        bridge_rules: &'a [crate::BridgeRule],
        material_mapping: rustc_hash::FxHashMap<compact_str::CompactString, u8>,
    ) -> Self {
        Self {
            voxel_size_z_nm,
            pours,
            contacts,
            substrate_layers,
            bridge_rules,
            material_mapping,
        }
    }

    /// Build conductive islands using flood-fill algorithm.
    pub fn build_conductive_islands(
        &self,
        pin_positions: Option<&[PinPosition]>,
    ) -> Vec<ConductiveIsland> {
        let builder = IslandBuilder::new(
            self.voxel_size_z_nm,
            self.pours,
            self.contacts,
            self.substrate_layers,
            self.bridge_rules,
            &self.material_mapping,
        );
        builder.build_islands(pin_positions)
    }

    /// Bind logical nets to physical islands.
    ///
    /// # Returns
    /// Vector of net-to-island bindings
    pub fn bind_nets_to_islands(&self, islands: &[ConductiveIsland]) -> Vec<NetIslandBinding> {
        let binder = NetBinder::new(self.pours, self.contacts, self.substrate_layers);
        binder.bind_nets(islands)
    }

    /// Validate physical continuity and detect violations.
    ///
    /// # Arguments
    /// * `islands` - All conductive islands built by flood-fill
    /// * `bindings` - Net-to-island bindings
    /// * `enable_p43` - Whether to check for floating conductors (requires pin detection)
    ///
    /// # Returns
    /// Vector of physical continuity violations
    pub fn validate_continuity(
        &self,
        islands: &[ConductiveIsland],
        bindings: &[NetIslandBinding],
        enable_p43: bool,
    ) -> Vec<PhysicalContinuityViolation> {
        let validator = ContinuityValidator::new(
            self.voxel_size_z_nm,
            self.pours,
            self.contacts,
            self.substrate_layers,
        );
        validator.validate(islands, bindings, enable_p43)
    }
}

#[cfg(test)]
mod tests;
