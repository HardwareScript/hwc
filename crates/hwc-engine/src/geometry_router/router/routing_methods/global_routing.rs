use super::super::super::pathfinding::route_net_deterministic;
use super::super::super::types::{NetRoute, RouteResult, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::constraint_manager::LayerDirection;
use crate::geometry::Point3D;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    pub fn route_net_global(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        let net_constraints = self
            .constraints
            .get_net_constraints(route.net_id)
            .cloned()
            .unwrap_or_default();

        let clearance_zones = &self.constraints.clearance_zones;
        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        let max_valid_z = self.bounds.depth_nm - self.voxel_size_nm;
        let clamp_z = |p: Point3D| -> Point3D {
            let z = p.z.min(max_valid_z).max(0);
            Point3D::new(p.x, p.y, z)
        };
        let start = clamp_z(route.start);
        let goal = clamp_z(route.goal);

        let layer = (start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        let routing_params = crate::geometry_router::pathfinding::RoutingParams {
            net_id: route.net_id,
            constraints: &net_constraints,
            bounds: self.bounds,
            layer_direction,
            voxel_size: crate::space::VoxelSize {
                x_nm: self.voxel_size_nm,
                y_nm: self.voxel_size_nm,
                z_nm: self.voxel_size_nm,
            },
            clearance_zones,
            occupied_voxels: &occupied_set,
            voxel_grid: None,
            corridor: None,
            fixed_z_nm: None,
            exempt_components: &[],
            substrate_layers: None,
            is_high_speed_net: false,
        };

        match route_net_deterministic(start, goal, &routing_params) {
            Some(path) => {
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

                let unrolled_vias: Vec<_> = detected_vias
                    .iter()
                    .flat_map(|via| self.unroll_detected_via(via))
                    .collect();

                let mut placed_vias = Vec::new();
                for via in unrolled_vias {
                    if self.can_place_via(via.position, via.from_z_nm, via.to_z_nm) {
                        self.stamp_via(&via);
                        self.vias.push(via.clone());
                        placed_vias.push(via);
                    }
                }

                for point in &path {
                    self.occupied_voxels.insert(*point, route.net_id);
                    let (x, y, z) = crate::voxel_grid::VoxelGrid::nm_to_voxel(
                        *point,
                        &crate::space::VoxelSize {
                            x_nm: self.voxel_size_nm,
                            y_nm: self.voxel_size_nm,
                            z_nm: self.voxel_size_nm,
                        },
                    );
                    self.voxel_grid.set_occupied(
                        x,
                        y,
                        z,
                        2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    );
                }

                let mut final_path = path;
                if !final_path.is_empty() {
                    final_path[0] = route.start;
                    *final_path.last_mut().unwrap() = route.goal;
                }

                Ok(RoutedNet {
                    net_id: route.net_id,
                    path: final_path,
                    vias: placed_vias,
                })
            }
            None => Err(RoutingError::NoPathFound {
                net_id: route.net_id,
                start: route.start,
                goal: route.goal,
            }),
        }
    }

    pub fn route_all_nets_steiner_global(
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

            let routed = self.route_net_steiner_global(net_id, pins)?;
            result.paths.insert(net_id, routed.path);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

    pub(crate) fn route_net_steiner_global(
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

        let initial_route = NetRoute {
            net_id,
            start: pins[0],
            goal: pins[1],
        };
        let initial_routed = self.route_net_global(&initial_route)?;
        net_paths.push(initial_routed.path.clone());
        all_vias.extend(initial_routed.vias);

        for &pin in &pins[2..] {
            let target = self.find_nearest_target_on_net(pin, &net_paths);

            let sub_route = NetRoute {
                net_id,
                start: pin,
                goal: target,
            };

            match self.route_net_global(&sub_route) {
                Ok(routed) => {
                    net_paths.push(routed.path.clone());
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
                        let fallback_route = NetRoute {
                            net_id,
                            start: pin,
                            goal: fallback_target,
                        };
                        let fallback_routed = self.route_net_global(&fallback_route)?;
                        net_paths.push(fallback_routed.path.clone());
                        all_vias.extend(fallback_routed.vias);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        let mut merged_path = Vec::new();
        for segment in &net_paths {
            merged_path.extend(segment);
        }

        Ok(RoutedNet {
            net_id,
            path: merged_path,
            vias: all_vias,
        })
    }
}
