use super::super::super::types::{NetRoute, RouteResult, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Resolve the boundary docking port for a pin inside a component.
    ///
    /// Computes the best N/S/E/W boundary port on the pad's outer edge
    /// and returns an escape point exactly at the profile-defined clearance.
    /// All coordinates are calculated as absolute integer values — no magic numbers.
    ///
    /// # Arguments
    /// * `pin` - The pin location (center of pad)
    /// * `target` - The target location to route toward
    /// * `trace_width_nm` - Width of the routing trace (needed for proper clearance calculation)
    pub fn resolve_boundary_port(
        &self,
        entity_graph: &EntityGraph,
        pin: Point3D,
        target: Point3D,
        trace_width_nm: i64,
    ) -> Point3D {
        // Read fabrication constraints (zero-magic - no fallbacks)
        let fabrication = self
            .constraints
            .fabrication
            .as_ref()
            .expect("BUG: Fabrication constraints required for boundary port resolution");

        // v0.2.1: Technology from profile (required field)
        let strategy = fabrication.technology;
        let port_escape_clearance =
            strategy.port_escape_clearance(trace_width_nm, fabrication.min_trace_spacing_nm);

        // Try 1: component_metadata lookup (fast, exact)
        let maybe_bbox = entity_graph
            .point_in_component(pin.x, pin.y, pin.z)
            .and_then(|component_name| {
                entity_graph
                    .get_component_metadata()
                    .iter()
                    .find(|c| c.name == component_name)
                    .map(|c| c.bbox)
            });

        // Try 2: fallback to pour substrate layers (catches pads with no component metadata)
        let bbox = match maybe_bbox {
            Some(b) => b,
            None => match entity_graph.get_pour_bbox_at_position(pin.x, pin.y, pin.z) {
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

        let port = if dy.abs() >= port_escape_clearance && dy.abs() > dx.abs() / 4 {
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
        } else if dy >= 0 {
            crate::geometry_router::port_escape::CardinalPort::North
        } else {
            crate::geometry_router::port_escape::CardinalPort::South
        };

        let escape = crate::geometry_router::port_escape::calculate_rect_escape(
            &bbox,
            port,
            crate::geometry_router::port_escape::EdgeOffset::Center,
            trace_width_nm, // v0.1.9: Pass trace width for smart corner clamping
            port_escape_clearance, // v0.1.9: Match obstacle inflation formula
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
    pub fn route_net_global(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
    ) -> Result<RoutedNet, RoutingError> {
        // v0.1.9: Fail-Fast — trace width MUST be declared for this net.
        // No fallbacks to PDK minimum. The compiler is responsible for ensuring
        // every route has an explicit width or a valid default.
        let trace_width = self
            .net_trace_widths
            .get(&route.net_id)
            .copied()
            .ok_or_else(|| RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: format!(
                    "No trace width declared for net_id={}. Every route must have an explicit \
                     'width:' parameter or the space must provide a default trace width.",
                    route.net_id.raw()
                ),
            })?;

        // v0.1.9: Fabrication constraints still required for clearance rules
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile. \
                    Ensure a profile with 'trace:' and 'clearance:' constraints \
                    is declared in the space definition."
                    .into(),
            }
        })?;

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

        // v0.2.0: PhysicalTruth - Structural Solution (Path Stitching Pattern)
        // In ASIC mode, "layer:" is a routing preference, NOT a coordinate override.
        // We route on the preferred layer and stitch vertical segments to connect the pins.
        let start = clamp_coords(route.start);
        let goal = clamp_coords(route.goal);
        let target_z_preference = self.net_layer_targets.get(&route.net_id).copied();

        // Determine the search points for routing
        // If we have a layer preference, route on that layer, otherwise route at pin heights
        let (search_start, search_goal) = if let Some(target_z) = target_z_preference {
            eprintln!("  Routing Layer (preferred): Z={}nm", target_z);
            eprintln!(
                "  Strategy: Route on Z={}, then stitch vertical segments to pins",
                target_z
            );
            (
                Point3D::new(start.x, start.y, target_z),
                Point3D::new(goal.x, goal.y, target_z),
            )
        } else {
            eprintln!("  No layer preference - routing at pin heights");
            (start, goal)
        };

        let board_bounds = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let track_pitch = self.resolution_nm;
        let topo_router =
            TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);
        let exempt_net_ids = vec![route.net_id.raw() as usize];

        // Route on the search layer (either pin heights or preferred routing layer)
        let routing_path =
            if let Some(&(start_normal, goal_normal)) = self.net_normals.get(&route.net_id) {
                let escape_stub_nm = self
                    .net_escape_stubs
                    .get(&route.net_id)
                    .copied()
                    .expect("COMPILER BUG: Net has normals but no escape_stub");

                eprintln!(
                    "[GLOBAL ROUTING] Net {} using perpendicular escape: stub={}nm",
                    route.net_id.raw(),
                    escape_stub_nm
                );

                match topo_router.route_with_perpendicular_escape(
                    crate::geometry_router::topological_router::PerpendicularEscapeParams {
                        start: search_start,
                        target: search_goal,
                        start_normal,
                        target_normal: goal_normal,
                        escape_stub_nm,
                        obstacles: &spatial_index,
                        board_bounds: &board_bounds,
                        exempt_net_ids: &exempt_net_ids,
                    },
                ) {
                    Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                    _ => {
                        return Err(RoutingError::NoPathFound {
                            net_id: route.net_id,
                            start: route.start,
                            goal: route.goal,
                        });
                    }
                }
            } else {
                match topo_router.route_with_exemptions(
                    search_start,
                    search_goal,
                    &spatial_index,
                    &board_bounds,
                    &exempt_net_ids,
                ) {
                    Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                    _ => {
                        return Err(RoutingError::NoPathFound {
                            net_id: route.net_id,
                            start: route.start,
                            goal: route.goal,
                        });
                    }
                }
            };

        // v0.2.0: PATH STITCHING - Build the 3D path with vertical segments
        // Structure: [Physical Pin Start] -> [Routing Layer] -> ... -> [Routing Layer] -> [Physical Pin Goal]
        let mut final_path = Vec::new();

        // Add entry vertical segment if start pin is not on routing layer
        if start.z != search_start.z {
            final_path.push(start);
        }

        // Add the routed path on the target layer
        final_path.extend(routing_path);

        // Add exit vertical segment if goal pin is not on routing layer
        if goal.z != search_goal.z {
            final_path.push(goal);
        }

        // Extract vias from the stitched path - this will detect the Z-transitions
        let detected_vias = self.extract_vias_from_path(&final_path, route.net_id);

        // Unroll detected vias using the Native Via Resolver
        let unrolled_vias: Vec<_> = detected_vias
            .iter()
            .flat_map(|via| self.unroll_detected_via(via))
            .collect();

        let mut placed_vias = Vec::new();
        for via in unrolled_vias {
            if self.can_place_via(entity_graph, via.position, via.from_z_nm, via.to_z_nm) {
                self.stamp_via(entity_graph, &via);
                self.vias.push(via.clone());
                placed_vias.push(via);
            }
        }

        // Commit the stitched path to the EntityGraph
        entity_graph.register_route(
            route.net_id,
            &final_path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        for (i, p) in final_path.iter().enumerate().take(6) {
            eprintln!(
                "[ROUTE_NET_GLOBAL DEBUG]     [{}]: ({},{},{})",
                i, p.x, p.y, p.z
            );
        }
        if final_path.len() > 6 {
            eprintln!(
                "[ROUTE_NET_GLOBAL DEBUG]     ... and {} more points",
                final_path.len() - 6
            );
        }

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![final_path],
            vias: placed_vias,
        })
    }

    pub fn route_all_nets_steiner_global(
        &mut self,
        entity_graph: &mut EntityGraph,
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
            let routed = self.decompose_net_steiner(entity_graph, net_id, pins)?;
            result.paths.insert(net_id, routed.paths);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

    pub fn route_all_nets_explicit_global(
        &mut self,
        entity_graph: &mut EntityGraph,
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

                let routed = self.route_net_global(entity_graph, &route)?;

                if let Some(path) = routed.paths.first() {
                    eprintln!(
                        "[EXPLICIT_GLOBAL DEBUG]   Returned path length: {}",
                        path.len()
                    );
                    for (j, p) in path.iter().enumerate().take(4) {
                        eprintln!(
                            "[EXPLICIT_GLOBAL DEBUG]   path[{}]: ({},{},{})",
                            j, p.x, p.y, p.z
                        );
                    }
                    if path.len() > 4 {
                        eprintln!(
                            "[EXPLICIT_GLOBAL DEBUG]   ... and {} more points",
                            path.len() - 4
                        );
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

        for (net_id, path_segments) in &result.paths {
            eprintln!(
                "[EXPLICIT_GLOBAL DEBUG]   Net {:?}: {} segments",
                net_id,
                path_segments.len()
            );
            for (seg_idx, segment) in path_segments.iter().enumerate() {
                eprintln!(
                    "[EXPLICIT_GLOBAL DEBUG]     Segment {}: {} points",
                    seg_idx,
                    segment.len()
                );
                for (pt_idx, pt) in segment.iter().enumerate().take(2) {
                    eprintln!(
                        "[EXPLICIT_GLOBAL DEBUG]       Point {}: ({},{},{})",
                        pt_idx, pt.x, pt.y, pt.z
                    );
                }
            }
        }

        Ok(result)
    }
}
