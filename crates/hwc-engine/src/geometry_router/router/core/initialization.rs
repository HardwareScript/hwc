//! Initialization logic for GeometryRouter

use super::types::{GeometryRouter, RouterConfig};
use crate::constraint_manager::ConstraintRulebook;
use crate::geometry_router::bounding_box_tracker::BoundingBoxTracker;
use crate::geometry_router::neighbor_generation::GridBounds;
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

        // Create EntityGraph for spatial queries and component metadata
        let entity_graph = EntityGraph::new();

        Self {
            bounds,
            constraints,
            layer_directions,
            resolution_nm,
            material_registry,
            entity_graph,
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
            routing_material_id: 0,
            trace_width_nm: 100_000,
            net_trace_widths: FxHashMap::default(),
        }
    }
}
