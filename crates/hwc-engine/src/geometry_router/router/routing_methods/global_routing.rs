use super::super::super::pathfinding::route_net_deterministic;
use super::super::super::types::{NetRoute, RouteResult, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::constraint_manager::LayerDirection;
use crate::geometry::Point3D;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Resolve the boundary docking port for a pin inside a component.
    ///
    /// Given a pin coordinate (which may be at the center of a pad) and a target
    /// direction, computes the best N/S/E/W boundary port on the pad's outer edge
    /// and returns a point ONE VOXEL OUTSIDE that boundary for A* seeding.
    ///
    /// If the pin is not inside any component, returns the pin coordinate as-is.
    pub(crate) fn resolve_boundary_port(&self, pin: Point3D, target: Point3D) -> Point3D {
        // Try 1: component_metadata lookup (fast, exact)
        let maybe_bbox = self
            .voxel_grid
            .point_in_component(pin.x, pin.y, pin.z)
            .and_then(|component_name| {
                self.voxel_grid
                    .get_component_metadata()
                    .iter()
                    .find(|c| c.name == component_name)
                    .map(|c| c.bbox)
            });

        // Try 2: fallback to pour substrate layers (catches pads with no component metadata)
        let bbox = match maybe_bbox {
            Some(b) => b,
            None => match self.voxel_grid.get_pour_bbox_at_position(pin.x, pin.y, pin.z) {
                Some(b) => b,
                None => return pin,
            },
        };

        // v0.1.7: Boundary Persistence
        // If the pin is already exactly on the boundary of the bbox, do NOT re-resolve it.
        // This preserves user-specified percentages/offsets from the compiler.
        let on_x_boundary = (pin.x - bbox.min.x).abs() < 1000 || (pin.x - bbox.max.x).abs() < 1000;
        let on_y_boundary = (pin.y - bbox.min.y).abs() < 1000 || (pin.y - bbox.max.y).abs() < 1000;
        
        if on_x_boundary || on_y_boundary {
            // Already on a boundary, check if it's the CORRECT boundary based on target
            let dx = target.x - pin.x;
            let dy = target.y - pin.y;
            
            let is_correct_side = if dx.abs() >= dy.abs() {
                if dx >= 0 { (pin.x - bbox.max.x).abs() < 1000 } else { (pin.x - bbox.min.x).abs() < 1000 }
            } else {
                if dy >= 0 { (pin.y - bbox.max.y).abs() < 1000 } else { (pin.y - bbox.min.y).abs() < 1000 }
            };

            if is_correct_side {
                return pin;
            }
        }

        // Auto-select best port based on direction from pin to target
        // v0.1.7: Smart Auto-Port Heuristic (Multi-Segment Awareness)
        let dx = target.x - pin.x;
        let dy = target.y - pin.y;

        let port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
            // Significant vertical move: prefer North/South
            if dy >= 0 {
                crate::geometry_router::port_escape::CardinalPort::North
            } else {
                crate::geometry_router::port_escape::CardinalPort::South
            }
        } else if dx.abs() > 0 {
            // Primarily horizontal or small vertical move: prefer East/West
            if dx >= 0 {
                crate::geometry_router::port_escape::CardinalPort::East
            } else {
                crate::geometry_router::port_escape::CardinalPort::West
            }
        } else {
            // Pure vertical or zero move
            if dy >= 0 {
                crate::geometry_router::port_escape::CardinalPort::North
            } else {
                crate::geometry_router::port_escape::CardinalPort::South
            }
        };

        // clearance=0: return the exact boundary point on the pad edge
        let escape = crate::geometry_router::port_escape::calculate_rect_escape(
            &bbox,
            port,
            crate::geometry_router::port_escape::EdgeOffset::Center,
            0,
            0,
            pin.z,
        );

        /*
        eprintln!(
            "[BOUNDARY DOCK] pin ({:.3},{:.3},{:.3}) -> port {:?} at ({:.3},{:.3},{:.3})",
            pin.x as f64 / 1_000_000.0,
            pin.y as f64 / 1_000_000.0,
            pin.z as f64 / 1_000_000.0,
            port,
            escape.point.x as f64 / 1_000_000.0,
            escape.point.y as f64 / 1_000_000.0,
            escape.point.z as f64 / 1_000_000.0,
        );
        */

        escape.point
    }

    pub fn route_net_global(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        let net_constraints = self
            .constraints
            .get_net_constraints(route.net_id)
            .cloned()
            .unwrap_or_default();

        let clearance_zones = &self.constraints.clearance_zones;
        let occupied_map = &self.occupied_voxels;

        let max_valid_x = self.bounds.width_nm.saturating_sub(1);
        let max_valid_y = self.bounds.height_nm.saturating_sub(1);
        let max_valid_z = self.bounds.depth_nm.saturating_sub(1);
        let clamp_coords = |p: Point3D| -> Point3D {
            Point3D::new(
                p.x.min(max_valid_x).max(0),
                p.y.min(max_valid_y).max(0),
                p.z.min(max_valid_z).max(0),
            )
        };
        let start = clamp_coords(route.start);
        let goal = clamp_coords(route.goal);

        // Boundary-Docking: Detect which components contain the start/goal pins
        // so the A* can exempt them from the interior lockout.
        let mut exempt_components_vec: smallvec::SmallVec<[compact_str::CompactString; 4]> =
            smallvec::SmallVec::new();
        if let Some(start_comp) = self.voxel_grid.point_in_component(start.x, start.y, start.z) {
            exempt_components_vec.push(start_comp);
        }
        if let Some(goal_comp) = self.voxel_grid.point_in_component(goal.x, goal.y, goal.z) {
            if !exempt_components_vec.contains(&goal_comp) {
                exempt_components_vec.push(goal_comp);
            }
        }

        let layer = (start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        let fixed_z = if !self.is_manhattan {
            // v0.1.7: Use original un-clamped Z for fixed-plane routing to avoid
            // "roof" artifacts where the trace drops to the voxel center.
            Some(route.start.z)
        } else {
            None
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
            occupied_voxels: occupied_map,
            voxel_grid: Some(&self.voxel_grid),
            corridor: None,
            fixed_z_nm: fixed_z,
            exempt_components: &exempt_components_vec,
            substrate_layers: self.substrate_layers.as_deref(),
            is_high_speed_net: self.is_high_speed_net(route.net_id),
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
                    paths: vec![final_path],
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
            result.paths.insert(net_id, routed.paths);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

    /// Route all nets as explicit point-to-point segments.
    ///
    /// Unlike Steiner routing, this method treats each Vec<Point3D> as an
    /// independent path from path[0] to path[1]... to path[N].
    /// Segments sharing the same NetId are allowed to touch/overlap.
    pub fn route_all_nets_explicit_global(
        &mut self,
        segments: &[(NetId, Vec<Point3D>)],
    ) -> Result<RouteResult, RoutingError> {
        let mut result = RouteResult::new();

        for (net_id, points) in segments {
            if points.len() < 2 {
                continue;
            }

            let mut net_path = Vec::new();
            let mut net_vias = Vec::new();

            for i in 0..points.len() - 1 {
                let route = NetRoute {
                    net_id: *net_id,
                    start: points[i],
                    goal: points[i + 1],
                };

                let routed = self.route_net_global(&route)?;
                if let Some(path) = routed.paths.into_iter().next() {
                    if i > 0 && !net_path.is_empty() {
                        // Avoid duplicating the joint point
                        net_path.extend(path.into_iter().skip(1));
                    } else {
                        net_path.extend(path);
                    }
                }
                net_vias.extend(routed.vias);
            }

            result.paths.entry(*net_id).or_default().push(net_path);
            result.vias.extend(net_vias);
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

        // Resolve boundary ports for the first two pins
        let start_port = self.resolve_boundary_port(pins[0], pins[1]);
        let goal_port = self.resolve_boundary_port(pins[1], pins[0]);

        let initial_route = NetRoute {
            net_id,
            start: start_port,
            goal: goal_port,
        };
        let initial_routed = self.route_net_global(&initial_route)?;
        net_paths.push(initial_routed.paths.into_iter().next().unwrap_or_default());
        all_vias.extend(initial_routed.vias);

        for &pin in &pins[2..] {
            let target = self.find_nearest_target_on_net(pin, &net_paths);

            // Resolve boundary port for this pin toward the target
            let start_port = self.resolve_boundary_port(pin, target);
            // The target is a point on an existing path (already on a boundary or
            // in open space), so use it directly.
            let sub_route = NetRoute {
                net_id,
                start: start_port,
                goal: target,
            };

            match self.route_net_global(&sub_route) {
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
                        let fallback_goal =
                            self.resolve_boundary_port(fallback_target, pin);
                        let fallback_route = NetRoute {
                            net_id,
                            start: fallback_port,
                            goal: fallback_goal,
                        };
                        let fallback_routed = self.route_net_global(&fallback_route)?;
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
}
