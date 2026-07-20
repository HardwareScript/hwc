//! Configuration and context methods for GeometryRouter

use super::types::GeometryRouter;
use crate::geometry_router::routing_patterns::RoutingPattern;
use crate::geometry_router::substrate_types::SubstrateLayer;
use crate::geometry_router::types::RoutingHeuristics;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// v0.1.8: Set per-net routing pattern policies.
    pub fn set_route_net_policies(&mut self, policies: FxHashMap<NetId, RoutingPattern>) {
        self.route_net_policies = policies;
    }

    /// Set the routing context: material and trace width for all subsequent routes.
    pub fn set_routing_context(&mut self, material_id: u8, trace_width_nm: i64) {
        self.routing_material_id = material_id;
        self.trace_width_nm = trace_width_nm;
    }

    /// Set routing heuristic weights from the PDK profile.
    pub fn set_routing_heuristics(&mut self, heuristics: RoutingHeuristics) {
        self.config.routing_heuristics = Some(heuristics);
    }

    /// Get all vias placed during routing (for drill file export).
    pub fn get_vias(&self) -> &[crate::geometry_router::types::Via] {
        &self.vias
    }

    /// Add a copper pour to the router (for anti-pad generation).
    pub fn add_copper_pour(&mut self, net_id: NetId, z_bottom_nm: i64) {
        self.copper_pours.push(super::types::CopperPour {
            net_id,
            z_bottom_nm,
        });
    }

    /// Configure the router for ASIC (Manhattan) or PCB (Octilinear) mode.
    pub fn set_profile_mode(
        &mut self,
        is_manhattan: bool,
        profile_layers: Vec<String>,
        layer_z_positions: Vec<i64>,
        layer_materials: Vec<u8>,
    ) {
        self.config.is_manhattan = is_manhattan;
        self.config.profile_layers = profile_layers;
        self.config.layer_z_positions = layer_z_positions;
        self.config.layer_materials = layer_materials;

        // Configure the spatial index layer Z-ranges from the stackup
        if self.config.layer_z_positions.len() >= 2 {
            let mut z_ranges = Vec::with_capacity(self.config.layer_z_positions.len());
            for i in 0..self.config.layer_z_positions.len() {
                let z_min = self.config.layer_z_positions[i];
                let z_max = if i + 1 < self.config.layer_z_positions.len() {
                    self.config.layer_z_positions[i + 1]
                } else {
                    self.bounds.depth_nm
                };
                z_ranges.push((z_min, z_max));
            }
            self.entity_graph.set_spatial_layer_z_ranges(&z_ranges);
        }
    }

    /// v0.1.7: Set substrate layers and net frequencies for SI-aware routing.
    pub fn set_substrate_context(
        &mut self,
        substrate_layers: Vec<SubstrateLayer>,
        net_frequencies: FxHashMap<NetId, f64>,
    ) {
        self.substrate_layers = Some(substrate_layers);
        self.net_frequencies = net_frequencies;
    }

    /// v0.1.7: Check if a net is high-speed (≥1 GHz) based on stored frequencies.
    pub fn is_high_speed_net(&self, net_id: NetId) -> bool {
        self.net_frequencies
            .get(&net_id)
            .is_some_and(|&freq| freq >= 1_000_000_000.0)
    }

    /// Get current routing heuristics.
    pub fn routing_heuristics(&self) -> Option<&RoutingHeuristics> {
        self.config.routing_heuristics.as_ref()
    }
}
