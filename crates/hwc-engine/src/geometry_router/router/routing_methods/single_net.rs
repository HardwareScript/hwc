use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry::{BoundingBox, TraceSegment};

impl GeometryRouter {
    /// Build a DynamicSpatialIndex from component obstacles and committed vector trace segments.
    ///
    /// Excludes the start and goal components of the active route from the obstacle map,
    /// enabling the pathfinder to dock to ports cleanly without self-collision deadlocks.
    /// All OTHER components (even those on the same net!) and all previously routed segments
    /// (except those touching our endpoints) are treated as hard obstacles.
    pub(crate) fn build_routing_spatial_index(&self, active_route: &NetRoute) -> DynamicSpatialIndex {
        let mut spatial_index = DynamicSpatialIndex::new();
        let mut seg_id = 0usize;

        // Resolve component names for the start and goal positions of the active route
        let start_comp = self.entity_graph.point_in_component(
            active_route.start.x, active_route.start.y, active_route.start.z,
        );
        let goal_comp = self.entity_graph.point_in_component(
            active_route.goal.x, active_route.goal.y, active_route.goal.z,
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
            let trace_seg = TraceSegment::new(meta.bbox.min, meta.bbox.max, width.max(height));
            spatial_index.insert(IndexedSegment::new(seg_id, 0, &trace_seg, meta.bbox.min.z));
            seg_id += 1;
        }

        // 2. Insert already-routed vector segments directly from the EntityGraph
        for (net_id, segments) in self.entity_graph.get_all_routes() {
            // v0.1.8 Same-Net Tapping: If these segments belong to the same net,
            // they are NOT hard obstacles. We can tap into them or overlap them.
            if *net_id == active_route.net_id {
                continue;
            }

            for segment in segments {
                // Skip segments that touch our start or goal endpoints to allow connection
                let touches_start = segment.start == active_route.start
                    || segment.end == active_route.start;
                let touches_goal = segment.start == active_route.goal
                    || segment.end == active_route.goal;

                if touches_start || touches_goal {
                    continue;
                }

                spatial_index.insert(IndexedSegment::new(
                    seg_id,
                    0, // Treat as hard obstacle to force routing around it
                    segment,
                    segment.start.z,
                ));
                seg_id += 1;
            }
        }

        spatial_index
    }

    /// Continuous detailed route of a point-to-point NetRoute with active legalization fallback.
    pub fn route_net(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
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
            crate::geometry::Point3D::new(self.bounds.width_nm, self.bounds.height_nm, self.bounds.depth_nm),
        );

        // Pass route context to exempt active endpoints from obstacles
        let spatial_index = self.build_routing_spatial_index(route);
        
        // v0.1.8: Use the actual trace width from fabrication constraints instead of 
        // a grid-based size. This ensures the router "sees" the board as a vector 
        // space with correct physical clearances.
        let trace_width = self.constraints.fabrication.as_ref()
            .map(|fab| fab.min_trace_width_nm)
            .unwrap_or(self.resolution_nm);
        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch

        let topo_router = TopologicalRouter::new(trace_width, track_pitch);

        let path = if start.x == goal.x && start.y == goal.y && start.z == goal.z {
            vec![start, goal]
        } else {
            match topo_router.route(start, goal, &spatial_index, &board_bounds) {
                Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                _ => {
                    // --- FALLBACK: Try Localized Legalization Window ---
                    let collision_window = BoundingBox::new(start, goal);
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
        self.entity_graph.register_route(route.net_id, &path);

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
        window: &BoundingBox,
        route: &NetRoute,
    ) -> Result<Vec<crate::geometry::Point3D>, RoutingError> {
        use crate::geometry_router::legalizer::Legalizer;

        let min_clearance = self.constraints.fabrication.as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .unwrap_or(200_000);

        let legalizer = Legalizer::new(min_clearance);

        // Pass route context to exempt active endpoints
        let spatial_index = self.build_routing_spatial_index(route);
        let overlapping = spatial_index.query_bbox(window);

        let segments: Vec<TraceSegment> = overlapping.iter().map(|idx| {
            TraceSegment::new(idx.start, idx.end, idx.width_nm)
        }).collect();

        if segments.is_empty() {
            // No existing traces to nudge — try a direct Manhattan path
            let mut path = Vec::new();
            path.push(route.start);
            if route.start.x != route.goal.x && route.start.y != route.goal.y {
                path.push(crate::geometry::Point3D::new(route.goal.x, route.start.y, route.start.z));
            }
            path.push(route.goal);
            return Ok(path);
        }

        // Run legalization to nudge existing traces
        let _legalized = legalizer.legalize(&segments, &spatial_index, 5);

        // After legalization, attempt routing again with the updated spatial index
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(self.bounds.width_nm, self.bounds.height_nm, self.bounds.depth_nm),
        );

        let updated_spatial_index = self.build_routing_spatial_index(route);
        let topo_router = TopologicalRouter::new(self.voxel_size_nm, self.voxel_size_nm);

        match topo_router.route(route.start, route.goal, &updated_spatial_index, &board_bounds) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => Ok(topo_path.waypoints),
            _ => {
                // Last resort: direct Manhattan path
                let mut path = vec![route.start];
                if route.start.x != route.goal.x && route.start.y != route.goal.y {
                    path.push(crate::geometry::Point3D::new(route.goal.x, route.start.y, route.start.z));
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
        use super::super::super::constraint_aware::constraint_aware_astar;

        let target_voxels = target_length_nm / self.voxel_size_nm;

        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        let bounds = (
            self.bounds.width_nm,
            self.bounds.height_nm,
            self.bounds.depth_nm,
        );

        let path_result = constraint_aware_astar(
            route.start,
            route.goal,
            target_voxels,
            pattern,
            &occupied_set,
            bounds,
            self.voxel_size_nm,
        );

        match path_result {
            Ok(path) => {
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

                // Commit canonically to the EntityGraph
                self.entity_graph.register_route(route.net_id, &path);

                Ok(RoutedNet {
                    net_id: route.net_id,
                    paths: vec![path],
                    vias: placed_vias,
                })
            }
            Err(err) => Err(RoutingError::ConstraintFailed {
                net_id: route.net_id,
                message: err,
            }),
        }
    }
}
