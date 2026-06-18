use super::super::super::types::{NetRoute, RouteResult, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::Point3D;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    pub fn find_nearest_target_on_net(
        &self,
        new_pin: Point3D,
        existing_paths: &[Vec<Point3D>],
    ) -> Point3D {
        if existing_paths.is_empty() {
            return new_pin;
        }

        existing_paths
            .iter()
            .flatten()
            .min_by_key(|&&pt| {
                let dx = pt.x - new_pin.x;
                let dy = pt.y - new_pin.y;
                let dz = pt.z - new_pin.z;
                dx * dx + dy * dy + dz * dz
            })
            .copied()
            .unwrap_or(new_pin)
    }

    pub fn route_net_steiner(
        &mut self,
        net_id: NetId,
        pins: &[Point3D],
    ) -> Result<RoutedNet, RoutingError> {
        if pins.len() < 2 {
            return Err(RoutingError::NoPathFound {
                net_id,
                start: pins[0],
                goal: pins[0],
            });
        }

        let mut net_paths: Vec<Vec<Point3D>> = Vec::new();
        let mut all_vias = Vec::new();

        // Resolve boundary ports for the first two pins
        let start_port = self.resolve_boundary_port(pins[0], pins[1]);
        let goal_port = self.resolve_boundary_port(pins[1], pins[0]);

        let initial_route = NetRoute {
            net_id,
            start: start_port,
            goal: goal_port,
        };
        let initial_routed = self.route_net(&initial_route)?;
        net_paths.push(initial_routed.paths.into_iter().next().unwrap_or_default());
        all_vias.extend(initial_routed.vias);

        for &pin in &pins[2..] {
            let target = self.find_nearest_target_on_net(pin, &net_paths);

            let start_port = self.resolve_boundary_port(pin, target);
            let sub_route = NetRoute {
                net_id,
                start: start_port,
                goal: target,
            };

            match self.route_net(&sub_route) {
                Ok(routed) => {
                    net_paths.push(routed.paths.into_iter().next().unwrap_or_default());
                    all_vias.extend(routed.vias);
                }
                Err(RoutingError::NoPathFound { .. }) => {
                    let mut best_fallback: Option<(Point3D, i64)> = None;
                    for &original_pin in pins.iter().take(pins.len()) {
                        let dx = original_pin.x - pin.x;
                        let dy = original_pin.y - pin.y;
                        let dz = original_pin.z - pin.z;
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        if best_fallback.is_none() || dist_sq < best_fallback.unwrap().1 {
                            best_fallback = Some((original_pin, dist_sq));
                        }
                    }

                    if let Some((fallback_target, _)) = best_fallback {
                        let fallback_port = self.resolve_boundary_port(pin, fallback_target);
                        let fallback_route = NetRoute {
                            net_id,
                            start: fallback_port,
                            goal: fallback_target,
                        };
                        let fallback_routed = self.route_net(&fallback_route)?;
                        net_paths.push(fallback_routed.paths.into_iter().next().unwrap_or_default());
                        all_vias.extend(fallback_routed.vias);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(RoutedNet {
            net_id,
            paths: net_paths,
            vias: all_vias,
        })
    }

    pub fn route_all_nets_steiner_internal(
        &mut self,
        nets: &FxHashMap<NetId, Vec<Point3D>>,
    ) -> Result<RouteResult, RoutingError> {
        let mut result = RouteResult::new();

        let mut sorted_nets: Vec<_> = nets.iter().collect();
        sorted_nets.sort_by_key(|(id, _)| id.0);

        for (&net_id, pins) in &sorted_nets {
            if pins.len() < 2 {
                continue;
            }

            let routed = self.route_net_steiner(net_id, pins)?;
            result.paths.insert(net_id, routed.paths);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }
}
