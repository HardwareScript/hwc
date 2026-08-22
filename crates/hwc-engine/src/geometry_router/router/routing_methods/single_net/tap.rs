use crate::geometry_router::router::core::GeometryRouter;
use crate::geometry_router::types::{NetRoute, RoutedNet, RoutingError};
use crate::geometry_router::EntityGraph;

impl GeometryRouter {
    /// v0.2.0 Hierarchical Routing: Route by tapping into existing same-net segments.
    ///
    /// This implements the fundamental hierarchical routing pattern where child routes
    /// from lower hierarchy levels become tap points for parent-level routing.
    ///
    /// Algorithm:
    /// 1. Find all tap points on existing segments (sample points along each segment)
    /// 2. Find the closest tap point to start
    /// 3. Find the closest tap point to goal
    /// 4. Route: start → tap_point_1 → (existing segment) → tap_point_2 → goal
    pub(super) fn route_with_tapping(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
        existing_segments: &[hwc_physics::TraceSegment],
    ) -> Result<RoutedNet, RoutingError> {
        use crate::geometry::Point3D;

        eprintln!(
            "[TAP ROUTING] Starting tap routing for NetId({})",
            route.net_id.raw()
        );
        eprintln!("[TAP ROUTING]   Start: {:?}", route.start);
        eprintln!("[TAP ROUTING]   Goal:  {:?}", route.goal);
        eprintln!(
            "[TAP ROUTING]   {} existing segments",
            existing_segments.len()
        );

        // Step 1: Extract all tap points from existing segments
        let mut tap_points = Vec::new();
        for (seg_idx, segment) in existing_segments.iter().enumerate() {
            // Add segment endpoints as tap points
            tap_points.push((segment.start, seg_idx, "start"));
            tap_points.push((segment.end, seg_idx, "end"));

            // Sample points along the segment for better connectivity
            let segment_length = ((segment.end.x - segment.start.x).pow(2)
                + (segment.end.y - segment.start.y).pow(2)
                + (segment.end.z - segment.start.z).pow(2)) as f64;
            let segment_length = segment_length.sqrt() as i64;

            if segment_length > 1000 {
                // Sample at 1µm intervals for long segments
                let num_samples = (segment_length / 1000).min(10) as usize;
                for i in 1..num_samples {
                    let t = i as f64 / num_samples as f64;
                    let sample_point = Point3D::new(
                        segment.start.x + ((segment.end.x - segment.start.x) as f64 * t) as i64,
                        segment.start.y + ((segment.end.y - segment.start.y) as f64 * t) as i64,
                        segment.start.z + ((segment.end.z - segment.start.z) as f64 * t) as i64,
                    );
                    tap_points.push((sample_point, seg_idx, "sample"));
                }
            }
        }

        eprintln!("[TAP ROUTING] Generated {} tap points", tap_points.len());

        // Step 2: Find closest tap point to start
        let closest_to_start = tap_points.iter().min_by_key(|(point, _, _)| {
            (point.x - route.start.x).pow(2)
                + (point.y - route.start.y).pow(2)
                + (point.z - route.start.z).pow(2)
        });

        // Step 3: Find closest tap point to goal
        let closest_to_goal = tap_points.iter().min_by_key(|(point, _, _)| {
            (point.x - route.goal.x).pow(2)
                + (point.y - route.goal.y).pow(2)
                + (point.z - route.goal.z).pow(2)
        });

        if let (
            Some((tap_start, seg_idx_start, pos_start)),
            Some((tap_goal, seg_idx_goal, pos_goal)),
        ) = (closest_to_start, closest_to_goal)
        {
            eprintln!(
                "[TAP ROUTING] Closest tap to start: {:?} (segment {}, {})",
                tap_start, seg_idx_start, pos_start
            );
            eprintln!(
                "[TAP ROUTING] Closest tap to goal:  {:?} (segment {}, {})",
                tap_goal, seg_idx_goal, pos_goal
            );

            // Step 4: Route start → tap_start
            let route_to_tap = NetRoute {
                net_id: route.net_id,
                start: route.start,
                goal: *tap_start,
                target_z: route.target_z,
                normals: route.normals,
                escape_stub_nm: route.escape_stub_nm,
            };

            // Use direct routing for tap connections (don't recurse)
            let result_to_tap = self.route_net_direct(entity_graph, &route_to_tap)?;

            // Step 5: Route tap_goal → goal
            let route_from_tap = NetRoute {
                net_id: route.net_id,
                start: *tap_goal,
                goal: route.goal,
                target_z: route.target_z,
                normals: route.normals,
                escape_stub_nm: route.escape_stub_nm,
            };

            let result_from_tap = self.route_net_direct(entity_graph, &route_from_tap)?;

            // Step 6: Combine paths
            // The existing segment between tap points is already in entity_graph,
            // so we just need to register our new segments
            eprintln!("[TAP ROUTING] Successfully routed via tapping!");
            eprintln!(
                "[TAP ROUTING]   Segment 1: {} waypoints (start → tap)",
                result_to_tap.paths[0].len()
            );
            eprintln!(
                "[TAP ROUTING]   Segment 2: {} waypoints (tap → goal)",
                result_from_tap.paths[0].len()
            );

            // Return the combined result
            let mut all_paths = result_to_tap.paths;
            all_paths.extend(result_from_tap.paths);

            let mut all_vias = result_to_tap.vias;
            all_vias.extend(result_from_tap.vias);

            Ok(RoutedNet {
                net_id: route.net_id,
                paths: all_paths,
                vias: all_vias,
            })
        } else {
            Err(RoutingError::NoPathFound {
                net_id: route.net_id,
                start: route.start,
                goal: route.goal,
            })
        }
    }

    /// Direct routing without tap-routing logic (to avoid recursion).
    fn route_net_direct(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
    ) -> Result<RoutedNet, RoutingError> {
        use crate::geometry::BoundingBox;
        use crate::geometry_router::topological_router::TopologicalRouter;

        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints".into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let track_pitch = self.manufacturing_grid_nm;

        let topo_router =
            TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

        let exempt_net_ids = vec![route.net_id.raw() as usize];

        let path = match topo_router.route_with_exemptions(
            route.start,
            route.goal,
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
