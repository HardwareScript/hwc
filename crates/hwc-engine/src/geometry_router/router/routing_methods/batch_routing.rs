use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry_router::EntityGraph;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    pub fn route_all_nets(
        &mut self,
        entity_graph: &mut EntityGraph,
        nets: &[NetRoute],
    ) -> Result<Vec<RoutedNet>, RoutingError> {
        let mut routed_nets = Vec::with_capacity(nets.len());

        for net in nets {
            let routed = self.route_net(entity_graph, net)?;

            routed_nets.push(routed);
        }

        Ok(routed_nets)
    }

    /// Route all nets sorted by priority (highest first).
    ///
    /// Priorities come from the PDK profile's `routing.net_priorities` block.
    /// Nets not declared in the profile get priority 0 (lowest).
    pub fn route_all_nets_with_priority(
        &mut self,
        entity_graph: &mut EntityGraph,
        nets: &[NetRoute],
        priorities: &FxHashMap<crate::netlist::NetId, u8>,
    ) -> Result<Vec<RoutedNet>, RoutingError> {
        let mut sorted_nets: Vec<NetRoute> = nets.to_vec();
        sorted_nets.sort_by(|a, b| {
            let pa = priorities.get(&a.net_id).copied().unwrap_or(0);
            let pb = priorities.get(&b.net_id).copied().unwrap_or(0);
            pb.cmp(&pa) // highest first
        });

        let mut routed_map = FxHashMap::default();
        for net in sorted_nets {
            let routed = self.route_net(entity_graph, &net)?;
            routed_map.insert(net.net_id, routed);
        }

        let mut result = Vec::with_capacity(nets.len());
        for net in nets {
            if let Some(routed) = routed_map.get(&net.net_id) {
                result.push(routed.clone());
            }
        }

        Ok(result)
    }
}
