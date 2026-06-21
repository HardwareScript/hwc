//! Physical Continuity Validation - Layer 3 of the Triple-Check Architecture
//!
//! Vector-first conductive walk that verifies physical connectivity by
//! walking through actual conductive geometry (substrate layers and
//! route segments), not just checking if names match or boxes touch.
//!
//! ## Algorithm
//!
//! ```text
//! 1. Collect all geometry nodes (substrate layers + route segments)
//! 2. Build spatial grid index for O(1) neighbor lookups
//! 3. For each unvisited node:
//!    a. Start a new island
//!    b. Walk to all physically-touching nodes (same material)
//!    c. Mark all visited nodes as part of this island
//! 4. Bind logical nets to physical islands
//! 5. Validate:
//!    - Each net has exactly 1 island (no disconnections) — P41
//!    - Each island has exactly 1 net (no shorts) — P42
//!    - Each island has at least 1 pin (no floating conductors) — P43
//! ```

mod island_builder;
mod net_binding;
mod spatial_grid;
mod types;
mod validation;

pub use island_builder::IslandBuilder;
pub use net_binding::NetBinder;
pub use types::*;
pub use validation::ContinuityValidator;

use crate::connectivity::SubstrateLayerMetadata;

pub struct PhysicalContinuityChecker<'a> {
    substrate_layers: &'a [SubstrateLayerMetadata],
    route_segments: &'a [RouteSegmentMetadata],
    bridge_rules: &'a [crate::BridgeRule],
    material_mapping: rustc_hash::FxHashMap<compact_str::CompactString, u8>,
}

impl<'a> PhysicalContinuityChecker<'a> {
    pub fn new(
        substrate_layers: &'a [SubstrateLayerMetadata],
        route_segments: &'a [RouteSegmentMetadata],
        bridge_rules: &'a [crate::BridgeRule],
        material_mapping: rustc_hash::FxHashMap<compact_str::CompactString, u8>,
    ) -> Self {
        Self {
            substrate_layers,
            route_segments,
            bridge_rules,
            material_mapping,
        }
    }

    pub fn build_conductive_islands(
        &self,
        pin_positions: Option<&[PinPosition]>,
    ) -> Vec<ConductiveIsland> {
        let builder = IslandBuilder::new(
            self.substrate_layers,
            self.route_segments,
            self.bridge_rules,
            &self.material_mapping,
        );
        builder.build_islands(pin_positions)
    }

    pub fn bind_nets_to_islands(&self, islands: &[ConductiveIsland]) -> Vec<NetIslandBinding> {
        NetBinder::bind_nets_from_substrates(
            self.substrate_layers,
            self.route_segments,
            islands,
        )
    }

    pub fn validate_continuity(
        &self,
        islands: &[ConductiveIsland],
        bindings: &[NetIslandBinding],
        enable_p43: bool,
    ) -> Vec<PhysicalContinuityViolation> {
        let validator = ContinuityValidator::new();
        validator.validate(islands, bindings, enable_p43)
    }
}

#[cfg(test)]
mod tests;
