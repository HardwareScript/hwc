use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;

impl GeometryRouter {
    pub fn route_all_nets(&mut self, nets: &[NetRoute]) -> Result<Vec<RoutedNet>, RoutingError> {
        let mut routed_nets = Vec::with_capacity(nets.len());

        for net in nets {
            let routed = self.route_net(net)?;

            routed_nets.push(routed);
        }

        Ok(routed_nets)
    }

    pub fn route_all_nets_with_priority(
        &mut self,
        nets: &[NetRoute],
        netlist: &crate::netlist::NetlistArena,
    ) -> Result<Vec<RoutedNet>, RoutingError> {
        use super::super::super::priority::NetPriority;
        use rustc_hash::FxHashMap;

        let mut priorities = FxHashMap::default();
        for net in nets {
            if let Some(net_data) = netlist.get_net(net.net_id) {
                let priority = NetPriority::from_net_name(&net_data.name);
                priorities.insert(net.net_id, priority);
            } else {
                priorities.insert(net.net_id, NetPriority::LowSpeed);
            }
        }

        let mut sorted_nets: Vec<NetRoute> = nets.to_vec();
        sorted_nets.sort_by(|a, b| {
            let priority_a = priorities.get(&a.net_id).unwrap();
            let priority_b = priorities.get(&b.net_id).unwrap();
            priority_b.cmp(priority_a)
        });

        let mut routed_map = FxHashMap::default();
        for net in sorted_nets {
            let routed = self.route_net(&net)?;
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
