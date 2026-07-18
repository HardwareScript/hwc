use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::{BoundingBox, TraceSegment};
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::topological_router::TopologicalRouter;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Build a DynamicSpatialIndex from component obstacles and committed vector trace segments.
    ///
    /// Excludes the start and goal components of the active route from the obstacle map,
    /// enabling the pathfinder to dock to ports cleanly without self-collision deadlocks.
    /// All OTHER components (even those on the same net!) and all previously routed segments
    /// (except those touching our endpoints) are treated as hard obstacles.
    pub(crate) fn build_routing_spatial_index(
        &self,
        active_route: &NetRoute,
    ) -> DynamicSpatialIndex {
        let mut spatial_index = DynamicSpatialIndex::new();
        
        eprintln!("[SPATIAL INDEX DEBUG] build_routing_spatial_index: Creating NEW spatial index for net {}", active_route.net_id.raw());

        // Configure layer Z-ranges from the stackup for layered queries
        if self.layer_z_positions.len() >= 2 {
            let mut z_ranges = Vec::with_capacity(self.layer_z_positions.len());
            for i in 0..self.layer_z_positions.len() {
                let z_min = self.layer_z_positions[i];
                let z_max = if i + 1 < self.layer_z_positions.len() {
                    self.layer_z_positions[i + 1]
                } else {
                    self.bounds.depth_nm
                };
                z_ranges.push((z_min, z_max));
            }
            spatial_index.set_layer_z_ranges(&z_ranges);
        }

        let mut seg_id = 0usize;

        // Resolve component names for the start and goal positions of the active route
        let start_comp = self.entity_graph.point_in_component(
            active_route.start.x,
            active_route.start.y,
            active_route.start.z,
        );
        let goal_comp = self.entity_graph.point_in_component(
            active_route.goal.x,
            active_route.goal.y,
            active_route.goal.z,
        );

        // 1. Insert component boundaries as hard obstacles (Net ID 0 / unconnected)
        for meta in self.entity_graph.get_component_metadata() {
            // EXEMPTION GUARD: Only exclude the start and goal components of the active route.
            // Even if other components are on the same net, they are physical obstacles
            // that should be routed around unless we are specifically tapping into them.
            if start_comp.as_deref() == Some(meta.name.as_str())
                || goal_comp.as_deref() == Some(meta.name.as_str())
            {
                continue;
            }

            let width = meta.bbox.max.x - meta.bbox.min.x;
            let height = meta.bbox.max.y - meta.bbox.min.y;
            let trace_seg = TraceSegment::new(
                meta.bbox.min,
                meta.bbox.max,
                width.max(height),
                meta.material,
            );
            let thickness_nm = meta.bbox.max.z - meta.bbox.min.z;
            let component_net_id = meta.net_bindings.values().next().copied().unwrap_or(0) as usize;
            spatial_index.insert(IndexedSegment {
                source: hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance {
                    instance_id: seg_id,
                },
                segment_id: seg_id,
                net_id: component_net_id,
                width_nm: 0,
                thickness_nm,
                start: trace_seg.start,
                end: trace_seg.end,
                layer: meta.bbox.min.z,
            });
            seg_id += 1;
        }

        // 2. Insert substrate layers (pours) as hard obstacles.
        // Pours belong to specific nets; routes must not pass through pours of other nets.
        // v0.1.9: Planes without nets (net_id = 0) are keepout zones and MUST be obstacles.
        
        // v0.1.9: Use self.substrate_layers (populated by route_space) instead of
        // entity_graph.get_substrate_layers() which is empty during routing.
        let substrate_layer_count = self.substrate_layers.as_ref().map(|v| v.len()).unwrap_or(0);
        
        eprintln!(
            "[SPATIAL INDEX DEBUG] build_routing_spatial_index called for net_id={}, found {} substrate layers",
            active_route.net_id.raw(),
            substrate_layer_count
        );
        
        if let Some(substrate_layers) = &self.substrate_layers {
            for (substrate_idx, sub_layer) in substrate_layers.iter().enumerate() {
                let sub_net_id = sub_layer.net;
                
                eprintln!(
                    "[SPATIAL INDEX DEBUG] Checking substrate layer: net={}, bbox=({},{},{}) to ({},{},{})",
                    sub_net_id,
                    sub_layer.bbox.min.x, sub_layer.bbox.min.y, sub_layer.bbox.min.z,
                    sub_layer.bbox.max.x, sub_layer.bbox.max.y, sub_layer.bbox.max.z
                );
                
                // Same-net pours are not obstacles (we can route over our own pours)
                // BUT: net_id = 0 pours (keepout zones) are ALWAYS obstacles
                if sub_net_id != 0 && crate::netlist::NetId(sub_net_id) == active_route.net_id {
                    eprintln!("[SPATIAL INDEX DEBUG]   ❌ Skipped (same net as active route)");
                    continue;
                }

                // DESTINATION PAD EXEMPTION (v0.1.9 C-Space fix):
                // The goal anchor is placed at pad_edge - trace_width/2, so it sits just
                // OUTSIDE the raw bbox. After Minkowski inflation by trace_width/2 the
                // destination pad's inflated boundary swallows the goal → EndPointOutsideSpace.
                // Fix: use trace_width/2 as a proximity margin. If the goal (or start) lands
                // within that margin of the bbox boundary, this layer is an endpoint pad and
                // must be exempted from the obstacle list.
                // Only keepout zones (net_id = 0) are always obstacles regardless.
                if sub_net_id != 0 {
                    let proximity = self.trace_width_nm / 2;
                    let goal = active_route.goal;
                    let bbox = &sub_layer.bbox;
                    // Expanded bbox by proximity margin on all XY sides (Z uses raw bounds)
                    if goal.x >= bbox.min.x - proximity && goal.x <= bbox.max.x + proximity
                        && goal.y >= bbox.min.y - proximity && goal.y <= bbox.max.y + proximity
                        && goal.z >= bbox.min.z && goal.z <= bbox.max.z
                    {
                        eprintln!("[SPATIAL INDEX DEBUG]   ❌ Skipped (destination pad — goal docks into boundary of this layer)");
                        continue;
                    }
                    // Also exempt if the start point is docking into this pad (different net-id source)
                    let start = active_route.start;
                    if start.x >= bbox.min.x - proximity && start.x <= bbox.max.x + proximity
                        && start.y >= bbox.min.y - proximity && start.y <= bbox.max.y + proximity
                        && start.z >= bbox.min.z && start.z <= bbox.max.z
                    {
                        eprintln!("[SPATIAL INDEX DEBUG]   ❌ Skipped (source pad — start docks into boundary of this layer)");
                        continue;
                    }
                }
                let width = sub_layer.bbox.max.x - sub_layer.bbox.min.x;
                let height = sub_layer.bbox.max.y - sub_layer.bbox.min.y;
                
                eprintln!(
                    "[SPATIAL INDEX DEBUG]   ✅ Adding as obstacle (width={}, height={})",
                    width, height
                );
                
                // Use a stable segment_id based on the substrate layer index
                // This ensures the SAME physical substrate layer always gets the SAME segment_id
                // across multiple builds of the spatial index
                let stable_segment_id = 1_000_000 + substrate_idx;
                
                spatial_index.insert(IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer {
                        index: substrate_idx,
                    },
                    segment_id: stable_segment_id,
                    net_id: sub_net_id as usize,
                    width_nm: 0,
                    thickness_nm: sub_layer.bbox.max.z - sub_layer.bbox.min.z,
                    start: sub_layer.bbox.min,
                    end: sub_layer.bbox.max,
                    layer: sub_layer.bbox.min.z,
                });
                seg_id += 1;
            }
        }

        // 3. Insert already-routed vector segments directly from the EntityGraph
        for (net_id, segments) in self.entity_graph.get_all_routes() {
            // v0.1.8 Same-Net Tapping: If these segments belong to the same net,
            // they are NOT hard obstacles. We can tap into them or overlap them.
            if *net_id == active_route.net_id {
                continue;
            }

            for (seg_idx, segment) in segments.iter().enumerate() {
                // Skip segments that touch our start or goal endpoints to allow connection
                let touches_start =
                    segment.start == active_route.start || segment.end == active_route.start;
                let touches_goal =
                    segment.start == active_route.goal || segment.end == active_route.goal;

                if touches_start || touches_goal {
                    continue;
                }

                spatial_index.insert(IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx: (*net_id).raw() as usize,
                        seg_idx,
                    },
                    segment_id: seg_idx,
                    net_id: (*net_id).raw() as usize,
                    width_nm: segment.width_nm,
                    thickness_nm: {
                        let mat_props = self
                            .material_registry
                            .get_material(segment.material_id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "FATAL: Route segment references unregistered material_id={}",
                                    segment.material_id
                                )
                            });
                        assert!(
                            mat_props.thickness_nm > 0,
                            "FATAL: Material id={} has zero thickness",
                            segment.material_id
                        );
                        mat_props.thickness_nm
                    },
                    start: segment.start,
                    end: segment.end,
                    layer: segment.start.z,
                });
                seg_id += 1;
            }
        }

        spatial_index
    }

    /// Continuous detailed route of a point-to-point NetRoute with active legalization fallback.
    pub fn route_net(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
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
        let spatial_index = self.build_routing_spatial_index(route);

        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch

        let path = if start.x == goal.x && start.y == goal.y && start.z == goal.z {
            vec![start, goal]
        } else {
            // v0.1.9: Use TopologicalRouter as the single authoritative routing engine
            let topo_router = TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);
            match topo_router.route(start, goal, &spatial_index, &board_bounds) {
                Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                _ => {
                    // --- FALLBACK: Try Localized Legalization Window ---
                    let collision_window = BoundingBox::new(start, goal);
                    if let Ok(legalized_coords) =
                        self.legalize_local_window(&collision_window, route)
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

        // Commit the resolved vector route canonically to the EntityGraph
        self.entity_graph.register_route(
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

    /// Localized legalization fallback: when topological routing fails, attempt to
    /// nudge adjacent traces within a bounding window to clear a path.
    pub(crate) fn legalize_local_window(
        &mut self,
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
        let all_routes = self.entity_graph.get_all_routes();
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
        let spatial_index = self.build_routing_spatial_index(route);

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

        let (legalized_segments, legalized_net_ids) = legalizer.legalize(
            &all_segments,
            &all_net_ids,
            &self.material_registry,
            &spatial_index,
            5,
        );

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
        let net_ids_to_clear: Vec<_> = self
            .entity_graph
            .get_all_routes()
            .iter()
            .map(|(net_id, _)| *net_id)
            .collect();
        for net_id in net_ids_to_clear {
            self.entity_graph.clear_routes_for_net(net_id);
        }
        for (idx, seg) in legalized_segments.iter().enumerate() {
            let net_id = legalized_net_ids[idx];
            self.entity_graph
                .register_trace_segments(net_id, vec![seg.clone()]);
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

        let updated_spatial_index = self.build_routing_spatial_index(route);
        let topo_router = TopologicalRouter::new(
            self.constraints
                .fabrication
                .as_ref()
                .expect("fabrication constraints guaranteed by earlier check")
                .min_trace_width_nm,
            self.resolution_nm,
            min_clearance,
        );

        match topo_router.route(
            route.start,
            route.goal,
            &updated_spatial_index,
            &board_bounds,
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

    pub fn route_net_with_length_constraint(
        &mut self,
        route: &NetRoute,
        target_length_nm: i64,
        pattern: &Option<super::super::super::routing_patterns::RoutingPattern>,
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

        let spatial_index = self.build_routing_spatial_index(route);
        let topo_router = TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

        let path = match topo_router.route(route.start, route.goal, &spatial_index, &board_bounds) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
            _ => {
                let collision_window = BoundingBox::new(route.start, route.goal);
                if let Ok(legalized_coords) = self.legalize_local_window(&collision_window, route) {
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
            let straight_length: i64 = path.windows(2)
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
                        if p_b.x > p_a.x { 0i64 } else { 180i64 }
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
                        let meander_length: i64 = meander_points.windows(2)
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
                            let scaled_meander = pat.generate_moves(mid, heading, scaled_step.max(trace_width));

                            // Splice the meander into the path
                            let mut new_path = Vec::with_capacity(path.len() + scaled_meander.len());
                            new_path.extend_from_slice(&path[..=best_seg_idx]);
                            // Skip first point of meander (it's the same as mid)
                            for pt in scaled_meander.iter().skip(1) {
                                new_path.push(*pt);
                            }
                            new_path.extend_from_slice(&path[best_seg_idx + 1..]);
                            new_path
                        } else {
                            // Meander length is close enough, splice it directly
                            let mut new_path = Vec::with_capacity(path.len() + meander_points.len());
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
            if self.can_place_via(via.position, via.from_z_nm, via.to_z_nm) {
                self.stamp_via(&via);
                self.vias.push(via.clone());
                placed_vias.push(via);
            }
        }

        self.entity_graph.register_route(
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
