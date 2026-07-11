use super::super::super::types::{NetRoute, RouteResult, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Resolve the boundary docking port for a pin inside a component.
    ///
    /// Computes the best N/S/E/W boundary port on the pad's outer edge
    /// and returns an escape point exactly at the profile-defined clearance.
    /// All coordinates are calculated as absolute integer values — no magic numbers.
    pub fn resolve_boundary_port(&self, pin: Point3D, target: Point3D) -> Point3D {
        // Read clearance in nanometers directly from fabrication constraints (zero-magic)
        // No fallback - if constraints are missing, this will panic with a clear error.
        let clearance_nm = self.constraints.fabrication
            .as_ref()
            .expect("BUG: Fabrication constraints required for boundary port resolution")
            .min_trace_spacing_nm;

        // Try 1: component_metadata lookup (fast, exact)
        let maybe_bbox = self
            .entity_graph
            .point_in_component(pin.x, pin.y, pin.z)
            .and_then(|component_name| {
                self.entity_graph
                    .get_component_metadata()
                    .iter()
                    .find(|c| c.name == component_name)
                    .map(|c| c.bbox)
            });

        // Try 2: fallback to pour substrate layers (catches pads with no component metadata)
        let bbox = match maybe_bbox {
            Some(b) => b,
            None => match self.entity_graph.get_pour_bbox_at_position(pin.x, pin.y, pin.z) {
                Some(b) => b,
                None => return pin,
            },
        };

        // v0.1.8: Strict Semantic Layer Abstraction.
        // The router targets the outer bounding box edge of the selected cardinal port
        // and terminates the path the instant it touches the edge.
        // Pad interiors are marked as impenetrable to prevent internal loops.
        
        // v0.1.8: Ensure the escape point maintains the EXACT Z-height of the pin.
        // This prevents the 'Z-mismatch' bug where traces would float above or below pads.
        let escape_z = pin.z;

        // Use resolution/snap-step for coordinate snapping.
        let _resolution_nm = self.resolution_nm; 
        
        // Calculate a proper cardinal port escape to ensure orthogonal "clean" entry
        // into the pad from the center of one of its faces.
        // The escape point is now outside the pad boundary by exactly one clearance_nm.
        
        // v0.1.8: Use the Bounding Box Center for direction calculation instead of
        // the pin position. This ensures perfectly orthogonal escapes even if the
        // pin (anchor) is slightly offset or if we're dealing with logical corners.
        let bbox_center_x = (bbox.min.x + bbox.max.x) / 2;
        let bbox_center_y = (bbox.min.y + bbox.max.y) / 2;

        let dx = target.x - bbox_center_x;
        let dy = target.y - bbox_center_y;

        let port = if dy.abs() >= clearance_nm && dy.abs() > dx.abs() / 4 {
            if dy >= 0 {
                crate::geometry_router::port_escape::CardinalPort::North
            } else {
                crate::geometry_router::port_escape::CardinalPort::South
            }
        } else if dx.abs() > 0 {
            if dx >= 0 {
                crate::geometry_router::port_escape::CardinalPort::East
            } else {
                crate::geometry_router::port_escape::CardinalPort::West
            }
        } else {
            if dy >= 0 {
                crate::geometry_router::port_escape::CardinalPort::North
            } else {
                crate::geometry_router::port_escape::CardinalPort::South
            }
        };

        let escape = crate::geometry_router::port_escape::calculate_rect_escape(
            &bbox,
            port,
            crate::geometry_router::port_escape::EdgeOffset::Center,
            0,
            clearance_nm, // BUG FIX: Don't add resolution_nm - clearance is already applied inside calculate_rect_escape
            escape_z,
            None, // v0.1.9: No board bounds available in legacy global routing context
        );

        // eprintln!(
        //     "[DEBUG PORT] Resolved port for pin {:?} targeting {:?}: port={:?}, escape={:?}",
        //     pin, target, port, escape.point
        // );

        escape.point
    }

    /// Global continuous router with localized active-set optimization fallback.
    pub fn route_net_global(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        // v0.1.8: Fail-Fast — fabrication constraints are MANDATORY.
        // No hardcoded fallbacks. All values come from the PDK profile.
        let fabrication = self.constraints.fabrication.as_ref()
            .ok_or_else(|| RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile. \
                    Ensure a profile with 'trace:' and 'clearance:' constraints \
                    is declared in the space definition.".into(),
            })?;

        let trace_width = fabrication.min_trace_width_nm;

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

        let board_bounds = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(self.bounds.width_nm, self.bounds.height_nm, self.bounds.depth_nm),
        );

        let spatial_index = self.build_routing_spatial_index(route);
        
        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch
        
        let topo_router = TopologicalRouter::new(trace_width, track_pitch);

        // v0.1.8: Prefer SDF-accelerated A* routing when an SDF generator is available.
        // The SDF router enforces guardrails (R25, Interior Lockout, Via-Portal Exemption).
        let path = if let Some(ref sdf) = self.sdf_generator {
            use crate::geometry_router::pathfinding::RoutingParams;
            use crate::geometry_router::pathfinding::route_net_sdf_accelerated;
            use crate::constraint_manager::LayerDirection;

                // v0.1.9: Empty layer routability map for engine internal routing
                let empty_layer_map = rustc_hash::FxHashMap::default();

                let routing_params = RoutingParams {
                    net_id: route.net_id,
                    constraints: &crate::constraint_manager::RouteConstraints {
                        min_trace_width_nm: trace_width,
                        min_clearance_nm: fabrication.min_trace_spacing_nm,
                        ..Default::default()
                    },
                    bounds: self.bounds.clone(),
                    layer_direction: LayerDirection::Any,
                    resolution_nm: self.resolution_nm,
                    clearance_zones: &[],
                    entity_graph: Some(&self.entity_graph),
                    // v0.1.8: Lock to exact physical Z when start and goal share the same Z.
                    // This prevents the SDF router from snapping Z to grid centers, which
                    // would destroy layer overrides (e.g. layer: metal1 → Z=300850).
                    fixed_z_nm: None,
                    exempt_components: &[],
                    substrate_layers: self.substrate_layers.as_deref(),
                    is_high_speed_net: false,
                    layer_routability_map: &empty_layer_map,
                    max_local_route_length_nm: None,
                    via_drill_diameter_nm: 0,
                    active_net_pin_positions: &[],
                    component_keepouts: &[],
                    // v0.1.8: Routing heuristic weights from PDK profile — fail if missing
                    base_cost: self.routing_heuristics.as_ref()
                        .ok_or_else(|| RoutingError::MissingFabricationConstraints {
                            net_id: route.net_id,
                            message: "Profile does not declare routing heuristics (base_cost, via_penalty, etc.). All routing weights must come from the PDK profile's 'routing:' block.".into(),
                        })?.base_cost,
                    via_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing via_penalty in profile routing heuristics.".into(),
                    })?.via_penalty,
                    direction_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing direction_penalty in profile routing heuristics.".into(),
                    })?.direction_penalty,
                    tight_clearance_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing tight_clearance_penalty in profile routing heuristics.".into(),
                    })?.tight_clearance_penalty,
                    crosstalk_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing crosstalk_penalty in profile routing heuristics.".into(),
                    })?.crosstalk_penalty,
                    impedance_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing impedance_penalty in profile routing heuristics.".into(),
                    })?.impedance_penalty,
                    reference_void_penalty: self.routing_heuristics.as_ref().ok_or_else(|| RoutingError::MissingFabricationConstraints {
                        net_id: route.net_id,
                        message: "Missing reference_void_penalty in profile routing heuristics.".into(),
                    })?.reference_void_penalty,
                };

            match route_net_sdf_accelerated(start, goal, &routing_params, sdf) {
                Some(sdf_path) if sdf_path.len() >= 2 => {
                    // SDF routing success logged for debugging
                    // eprintln!("[SDF-ROUTER] net {} routed via SDF ({} points)", route.net_id.raw(), sdf_path.len());
                    sdf_path
                }
                _ => {
                    // eprintln!("[SDF-ROUTER] net {} SDF failed, falling back to TopologicalRouter", route.net_id.raw());
                    match topo_router.route(start, goal, &spatial_index, &board_bounds) {
                        Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                        _ => {
                            let collision_window = BoundingBox::new(start, goal);
                            if let Ok(legalized_path) = self.legalize_local_window(&collision_window, route) {
                                legalized_path
                            } else {
                                return Err(RoutingError::NoPathFound {
                                    net_id: route.net_id,
                                    start: route.start,
                                    goal: route.goal,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            match topo_router.route(start, goal, &spatial_index, &board_bounds) {
                Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                _ => {
                    let collision_window = BoundingBox::new(start, goal);
                    if let Ok(legalized_path) = self.legalize_local_window(&collision_window, route) {
                        legalized_path
                    } else {
                        return Err(RoutingError::NoPathFound {
                            net_id: route.net_id,
                            start: route.start,
                            goal: route.goal,
                        });
                    }
                }
            }
        };

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

        // Commit canonically to the EntityGraph.
        self.entity_graph.register_route(
            route.net_id,
            &path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        let mut final_path = path;
        
        eprintln!("[ROUTE_NET_GLOBAL DEBUG] Path BEFORE boundary restore:");
        eprintln!("  route.start=({},{},{}), route.goal=({},{},{})", 
            route.start.x, route.start.y, route.start.z, route.goal.x, route.goal.y, route.goal.z);
        for (i, p) in final_path.iter().enumerate().take(4) {
            eprintln!("  final_path[{}]=({},{},{})", i, p.x, p.y, p.z);
        }
        if final_path.len() > 4 {
            eprintln!("  ... and {} more points", final_path.len() - 4);
        }
        
        if !final_path.is_empty() {
            final_path[0] = route.start;
            *final_path.last_mut().unwrap() = route.goal;
        }
        
        eprintln!("[ROUTE_NET_GLOBAL DEBUG] Path AFTER boundary restore:");
        for (i, p) in final_path.iter().enumerate().take(4) {
            eprintln!("  final_path[{}]=({},{},{})", i, p.x, p.y, p.z);
        }
        if final_path.len() > 4 {
            eprintln!("  ... and {} more points", final_path.len() - 4);
        }

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![final_path],
            vias: placed_vias,
        })
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

            // v0.1.8: Use the Steiner tree algorithm to decompose the net into
            // point-to-point global routes.
            let routed = self.decompose_net_steiner(net_id, pins)?;
            result.paths.insert(net_id, routed.paths);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

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

                eprintln!("[EXPLICIT DEBUG] net_id={}, start=({},{},{}), goal=({},{},{})", 
                    net_id.raw(), route.start.x, route.start.y, route.start.z,
                    route.goal.x, route.goal.y, route.goal.z);

                let routed = self.route_net_global(&route)?;
                
                if let Some(path) = routed.paths.first() {
                    eprintln!("[EXPLICIT DEBUG] Returned path ({} points):", path.len());
                    for (j, p) in path.iter().enumerate().take(4) {
                        eprintln!("  path[{}]: ({},{},{})", j, p.x, p.y, p.z);
                    }
                    if path.len() > 4 {
                        eprintln!("  ... and {} more points", path.len() - 4);
                    }
                }
                if let Some(path) = routed.paths.into_iter().next() {
                    if i > 0 && !net_path.is_empty() {
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
}
