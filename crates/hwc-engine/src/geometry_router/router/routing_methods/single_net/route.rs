use crate::geometry::BoundingBox;
use crate::geometry_router::router::core::GeometryRouter;
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::types::{NetRoute, RoutedNet, RoutingError};
use crate::geometry_router::EntityGraph;

impl GeometryRouter {
    /// Continuous detailed route of a point-to-point NetRoute with active legalization fallback.
    pub fn route_net(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
    ) -> Result<RoutedNet, RoutingError> {
        // v0.2.0 HIERARCHICAL ROUTING: Check if same-net segments already exist (child routes)
        // If they do, route to them as intermediate waypoints instead of direct routing
        // Do this BEFORE borrowing fabrication to avoid borrow checker conflicts
        let existing_segments = entity_graph
            .get_all_routes()
            .iter()
            .find(|(net_id, _)| *net_id == route.net_id)
            .map(|(_, segments)| segments.clone());

        if let Some(segments) = &existing_segments {
            if !segments.is_empty() {
                eprintln!(
                    "[HIERARCHICAL ROUTING] Found {} existing same-net segments for NetId({}) - attempting tap routing",
                    segments.len(),
                    route.net_id.raw()
                );

                // Try routing to tap into existing segments
                if let Ok(result) = self.route_with_tapping(entity_graph, route, segments) {
                    return Ok(result);
                }

                eprintln!(
                    "[HIERARCHICAL ROUTING] Tap routing failed, falling back to direct routing"
                );
            }
        }

        // v0.1.8: Fail-Fast — fabrication constraints are MANDATORY.
        // No hardcoded fallbacks. All values come from the PDK profile.
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile. \
                    Ensure a profile with 'trace:' and 'clearance:' constraints \
                    is declared in the space definition."
                    .into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let max_x = self.bounds.width_nm.saturating_sub(1);
        let max_y = self.bounds.height_nm.saturating_sub(1);
        let max_z = self.bounds.depth_nm.saturating_sub(1);
        let clamp_coord = |p: crate::geometry::Point3D| -> crate::geometry::Point3D {
            crate::geometry::Point3D::new(
                p.x.max(0).min(max_x),
                p.y.max(0).min(max_y),
                p.z.max(0).min(max_z),
            )
        };
        let start = clamp_coord(route.start);
        let goal = clamp_coord(route.goal);

        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        // Pass route context to exempt active endpoints from obstacles
        let spatial_index = self.build_routing_spatial_index(entity_graph, route);

        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch

        let path = if start.x == goal.x && start.y == goal.y && start.z == goal.z {
            vec![start, goal]
        } else {
            // v0.1.9: Use TopologicalRouter as the single authoritative routing engine
            let topo_router =
                TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

            // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
            // Exempt the active net_id so start/goal pads are not treated as obstacles
            let exempt_net_ids = vec![route.net_id.raw() as usize];

            // v0.1.9: TopologicalRouter is the single authoritative routing engine
            // NO FALLBACK - if TopologicalRouter can't find a path, the route fails
            match topo_router.route_with_exemptions(
                start,
                goal,
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

        let detected_vias = self.extract_vias_from_path(&path, route.net_id);

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

        // Commit the resolved vector route canonically to the EntityGraph
        entity_graph.register_route(
            route.net_id,
            &path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![path],
            vias: placed_vias,
        })
    }
}
