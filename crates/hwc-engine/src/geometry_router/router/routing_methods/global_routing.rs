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
        let clearance_nm = self.constraints.fabrication.as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .unwrap_or(200_000); // Default to 200um

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

        // Use resolution/snap-step for coordinate snapping instead of voxel grid cell size
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
            clearance_nm + self.resolution_nm, // Ensure we are truly outside the box
            escape_z,
        );

        eprintln!(
            "[DEBUG PORT] Resolved port for pin {:?} targeting {:?}: port={:?}, escape={:?}",
            pin, target, port, escape.point
        );

        escape.point
    }

    /// Global continuous router with localized active-set optimization fallback.
    pub fn route_net_global(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
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
        
        // v0.1.8: Use the actual trace width from fabrication constraints instead of 
        // a grid-based size. This ensures the router "sees" the board as a vector 
        // space with correct physical clearances.
        let trace_width = self.constraints.fabrication.as_ref()
            .map(|fab| fab.min_trace_width_nm)
            .unwrap_or(self.resolution_nm);
        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch
        
        let topo_router = TopologicalRouter::new(trace_width, track_pitch);

        match topo_router.route(start, goal, &spatial_index, &board_bounds) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => {
                let path = topo_path.waypoints;
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

                // Commit canonically to the EntityGraph (no occupied_voxels)
                self.entity_graph.register_route(route.net_id, &path);

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
            _ => {
                // --- FALLBACK: Localized Legalization and Compaction ---
                let collision_window = BoundingBox::new(start, goal);
                if let Ok(legalized_path) = self.legalize_local_window(&collision_window, route) {
                    self.entity_graph.register_route(route.net_id, &legalized_path);
                    Ok(RoutedNet {
                        net_id: route.net_id,
                        paths: vec![legalized_path],
                        vias: Vec::new(),
                    })
                } else {
                    Err(RoutingError::NoPathFound {
                        net_id: route.net_id,
                        start: route.start,
                        goal: route.goal,
                    })
                }
            }
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

                let routed = self.route_net_global(&route)?;
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
