use crate::geometry::BoundingBox;
use crate::geometry_router::router::core::GeometryRouter;
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::types::{NetRoute, RoutedNet, RoutingError};
use crate::geometry_router::EntityGraph;

impl GeometryRouter {
    pub fn route_net_with_length_constraint(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
        target_length_nm: i64,
        pattern: &Option<crate::geometry_router::routing_patterns::RoutingPattern>,
    ) -> Result<RoutedNet, RoutingError> {
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile.".into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let track_pitch = self.resolution_nm;
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let topo_router =
            TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

        // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
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
                let collision_window = BoundingBox::new(route.start, route.goal);
                if let Ok(legalized_coords) =
                    self.legalize_local_window(entity_graph, &collision_window, route)
                {
                    legalized_coords
                } else {
                    return Err(RoutingError::NoPathFound {
                        net_id: route.net_id,
                        start: route.start,
                        goal: route.goal,
                    });
                }
            }
        };

        // v0.1.9: If a pattern is provided and the straight path is shorter than
        // the target, inject meander at the midpoint of the longest straight segment.
        let final_path = if let Some(pat) = pattern {
            let straight_length: i64 = path
                .windows(2)
                .map(|w| {
                    let dx = (w[0].x - w[1].x).abs();
                    let dy = (w[0].y - w[1].y).abs();
                    let dz = (w[0].z - w[1].z).abs();
                    dx + dy + dz
                })
                .sum();

            let deficit = target_length_nm.saturating_sub(straight_length);
            if deficit > 0 {
                // Find the longest straight segment to place the meander
                let mut best_seg_idx = 0;
                let mut best_seg_len = 0i64;
                for (i, w) in path.windows(2).enumerate() {
                    let dx = (w[0].x - w[1].x).abs();
                    let dy = (w[0].y - w[1].y).abs();
                    let dz = (w[0].z - w[1].z).abs();
                    let seg_len = dx + dy + dz;
                    if seg_len > best_seg_len {
                        best_seg_len = seg_len;
                        best_seg_idx = i;
                    }
                }

                if best_seg_len > trace_width * 4 {
                    // Compute midpoint of the longest segment
                    let p_a = path[best_seg_idx];
                    let p_b = path[best_seg_idx + 1];
                    let mid = crate::geometry::Point3D::new(
                        (p_a.x + p_b.x) / 2,
                        (p_a.y + p_b.y) / 2,
                        (p_a.z + p_b.z) / 2,
                    );

                    // Determine heading from segment direction
                    let heading = if (p_a.x - p_b.x).abs() > (p_a.y - p_b.y).abs() {
                        if p_b.x > p_a.x {
                            0i64
                        } else {
                            180i64
                        }
                    } else if p_b.y > p_a.y {
                        90i64
                    } else {
                        270i64
                    };

                    // Generate meander waypoints from the pattern
                    let step_size = trace_width * 2;
                    let meander_points = pat.generate_moves(mid, heading, step_size);

                    if meander_points.len() > 2 {
                        // Calculate how much length the meander actually adds
                        let meander_length: i64 = meander_points
                            .windows(2)
                            .map(|w| {
                                let dx = (w[0].x - w[1].x).abs();
                                let dy = (w[0].y - w[1].y).abs();
                                dx + dy
                            })
                            .sum();

                        // Scale the meander if it's too long or too short
                        let scale = if meander_length > 0 {
                            (deficit as f64 / meander_length as f64).sqrt()
                        } else {
                            1.0
                        };

                        if (scale - 1.0).abs() > 0.1 {
                            // Re-generate with scaled step size
                            let scaled_step = (step_size as f64 * scale) as i64;
                            let scaled_meander =
                                pat.generate_moves(mid, heading, scaled_step.max(trace_width));

                            // Splice the meander into the path
                            let mut new_path =
                                Vec::with_capacity(path.len() + scaled_meander.len());
                            new_path.extend_from_slice(&path[..=best_seg_idx]);
                            // Skip first point of meander (it's the same as mid)
                            for pt in scaled_meander.iter().skip(1) {
                                new_path.push(*pt);
                            }
                            new_path.extend_from_slice(&path[best_seg_idx + 1..]);
                            new_path
                        } else {
                            // Meander length is close enough, splice it directly
                            let mut new_path =
                                Vec::with_capacity(path.len() + meander_points.len());
                            new_path.extend_from_slice(&path[..=best_seg_idx]);
                            for pt in meander_points.iter().skip(1) {
                                new_path.push(*pt);
                            }
                            new_path.extend_from_slice(&path[best_seg_idx + 1..]);
                            new_path
                        }
                    } else {
                        path
                    }
                } else {
                    path
                }
            } else {
                path
            }
        } else {
            path
        };

        let detected_vias = self.extract_vias_from_path(&final_path, route.net_id);

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

        // v0.2.1 FIX: Routes will get correct materials from routing database export
        entity_graph.register_route(
            route.net_id,
            &final_path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![final_path],
            vias: placed_vias,
        })
    }
}
