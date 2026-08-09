use crate::geometry::BoundingBox;
use crate::geometry_router::router::core::GeometryRouter;
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::types::{NetRoute, RoutingError};
use crate::geometry_router::EntityGraph;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Localized legalization fallback: when topological routing fails, attempt to
    /// nudge adjacent traces within a bounding window to clear a path.
    pub(crate) fn legalize_local_window(
        &mut self,
        entity_graph: &mut EntityGraph,
        _window: &BoundingBox,
        route: &NetRoute,
    ) -> Result<Vec<crate::geometry::Point3D>, RoutingError> {
        use crate::geometry_router::legalizer::Legalizer;

        // v0.1.8: Fail-Fast — fabrication constraints are MANDATORY.
        let min_clearance = self
            .constraints
            .fabrication
            .as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .ok_or_else(|| RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "Legalization requires fabrication constraints but none are loaded."
                    .into(),
            })?;

        let legalizer = Legalizer::new(min_clearance);

        // Collect all segments and net_ids from the entity graph (the source of truth)
        let all_routes = entity_graph.get_all_routes();
        let mut all_segments = Vec::new();
        let mut all_net_ids = Vec::new();
        for (net_id, segments) in all_routes {
            for seg in segments {
                all_segments.push(seg.clone());
                all_net_ids.push(*net_id);
            }
        }

        if all_segments.is_empty() {
            // No existing traces to nudge — try a direct Manhattan path
            let mut path = Vec::new();
            path.push(route.start);
            if route.start.x != route.goal.x && route.start.y != route.goal.y {
                path.push(crate::geometry::Point3D::new(
                    route.goal.x,
                    route.start.y,
                    route.start.z,
                ));
            }
            path.push(route.goal);
            return Ok(path);
        }

        // Run legalization to nudge existing traces
        let spatial_index = self.build_routing_spatial_index(entity_graph, route);

        // Record original positions to compute displacements for via sliding
        let original_centers: Vec<(crate::netlist::NetId, i64, i64)> = all_segments
            .iter()
            .zip(all_net_ids.iter())
            .map(|(seg, net_id)| {
                let cx = (seg.start.x + seg.end.x) / 2;
                let cy = (seg.start.y + seg.end.y) / 2;
                (*net_id, cx, cy)
            })
            .collect();

        let (legalized_segments, legalized_net_ids) =
            legalizer.legalize(&all_segments, &all_net_ids, &spatial_index, 5);

        // Compute per-net displacement from legalization and update via positions
        let mut net_displacements: FxHashMap<crate::netlist::NetId, (i64, i64)> =
            FxHashMap::default();
        let mut net_counts: FxHashMap<crate::netlist::NetId, usize> = FxHashMap::default();
        for (idx, seg) in legalized_segments.iter().enumerate() {
            let net_id = legalized_net_ids[idx];
            let new_cx = (seg.start.x + seg.end.x) / 2;
            let new_cy = (seg.start.y + seg.end.y) / 2;
            if let Some((orig_cx, orig_cy)) =
                original_centers.get(idx).map(|(_, cx, cy)| (*cx, *cy))
            {
                let entry = net_displacements.entry(net_id).or_insert((0, 0));
                entry.0 += new_cx - orig_cx;
                entry.1 += new_cy - orig_cy;
                *net_counts.entry(net_id).or_insert(0) += 1;
            }
        }
        // Average displacement per net
        for (net_id, count) in &net_counts {
            if let Some(disp) = net_displacements.get_mut(net_id) {
                if *count > 0 {
                    disp.0 /= *count as i64;
                    disp.1 /= *count as i64;
                }
            }
        }
        // Slide vias that belong to displaced nets
        for via in &mut self.vias {
            if let Some(&(dx, dy)) = net_displacements.get(&via.net_id) {
                if dx != 0 || dy != 0 {
                    via.position.0 += dx;
                    via.position.1 += dy;
                }
            }
        }

        // Write legalized segments back to EntityGraph (source of truth)
        let net_ids_to_clear: Vec<_> = entity_graph
            .get_all_routes()
            .iter()
            .map(|(net_id, _)| *net_id)
            .collect();
        for net_id in net_ids_to_clear {
            entity_graph.clear_routes_for_net(net_id);
        }
        for (idx, seg) in legalized_segments.iter().enumerate() {
            let net_id = legalized_net_ids[idx];
            entity_graph.register_trace_segments(net_id, vec![seg.clone()]);
        }

        // Rebuild spatial index from updated EntityGraph
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let updated_spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let topo_router = TopologicalRouter::new(
            self.constraints
                .fabrication
                .as_ref()
                .expect("fabrication constraints guaranteed by earlier check")
                .min_trace_width_nm,
            self.manufacturing_grid_nm,
            min_clearance,
        );

        // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
        let exempt_net_ids = vec![route.net_id.raw() as usize];

        match topo_router.route_with_exemptions(
            route.start,
            route.goal,
            &updated_spatial_index,
            &board_bounds,
            &exempt_net_ids,
        ) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => Ok(topo_path.waypoints),
            _ => {
                let mut path = vec![route.start];
                if route.start.x != route.goal.x && route.start.y != route.goal.y {
                    path.push(crate::geometry::Point3D::new(
                        route.goal.x,
                        route.start.y,
                        route.start.z,
                    ));
                }
                path.push(route.goal);
                Ok(path)
            }
        }
    }
}
