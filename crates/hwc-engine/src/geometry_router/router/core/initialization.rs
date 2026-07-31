//! Initialization logic for GeometryRouter

use super::types::{GeometryRouter, RouterConfig};
use crate::constraint_manager::ConstraintRulebook;
use crate::geometry_router::bounding_box_tracker::BoundingBoxTracker;
use crate::geometry_router::neighbor_generation::GridBounds;
use crate::geometry_router::pathfinding::CostComposer;
use crate::geometry_router::EntityGraph;
use crate::material::MaterialRegistry;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Create a new geometry router.
    ///
    /// # Arguments
    /// * `bounds` - Grid bounds for routing
    /// * `constraints` - Constraint rulebook from constraint manager
    /// * `material_registry` - Material registry for physical thickness lookups
    ///
    /// # Important (v0.2.0 Refactor)
    /// GeometryRouter no longer owns EntityGraph. Instead, routing methods take
    /// `&mut EntityGraph` as a parameter. The EntityGraph lives in Space and is
    /// the single source of truth.
    pub fn new(
        bounds: GridBounds,
        constraints: ConstraintRulebook,
        material_registry: MaterialRegistry,
    ) -> Self {
        let resolution_nm = constraints.resolution_nm;

        // Extract layer directions from constraints
        let num_layers = constraints.layer_directions.len();
        let layer_directions = (0..num_layers)
            .map(|i| constraints.get_layer_direction(i))
            .collect();

        Self {
            bounds,
            constraints,
            layer_directions,
            resolution_nm,
            material_registry,
            vias: Vec::new(),
            copper_pours: Vec::new(),
            bounding_box_tracker: BoundingBoxTracker::new(),
            config: RouterConfig {
                resolution_nm,
                ..Default::default()
            },
            substrate_layers: None,
            net_frequencies: FxHashMap::default(),
            partition_grid: None,
            query_store: None,
            route_net_policies: FxHashMap::default(),
            // v0.1.9 NOTE: These global routing context values are initialization placeholders.
            // The compiler MUST call set_routing_context() with proper values from the stackup
            // before invoking route_space(). Per-net trace widths override these in v0.1.9.
            routing_material_id: 0, // Placeholder - will be set by compiler from stackup
            trace_width_nm: 100_000, // Placeholder - will be set by compiler (max width)
            net_trace_widths: FxHashMap::default(),
            net_normals: FxHashMap::default(),
            net_escape_stubs: FxHashMap::default(),
            cost_composer: CostComposer::default(),
            intent_composers: FxHashMap::default(),
            net_layer_targets: FxHashMap::default(),
        }
    }

    /// v0.1.9: Register a cost composer for a named routing intent.
    pub fn register_intent_composer(
        &mut self,
        name: compact_str::CompactString,
        composer: CostComposer,
    ) {
        self.intent_composers.insert(name, composer);
    }

    /// v0.1.9: Get the cost composer for a named routing intent, falling back to default.
    pub fn get_intent_composer(&self, name: &str) -> &CostComposer {
        self.intent_composers
            .get(name)
            .unwrap_or(&self.cost_composer)
    }

    /// v0.1.9: Check if a named routing intent composer is registered.
    pub fn has_intent_composer(&self, name: &str) -> bool {
        self.intent_composers.contains_key(name)
    }
}
