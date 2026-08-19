//! Route registration and query methods for EntityGraph.

use crate::geometry::TraceSegment;
use crate::netlist::NetId;

use super::EntityGraph;

impl EntityGraph {
    /// Commit the current routing session (placeholder).
    pub fn commit_route(&mut self) {}

    /// Register a routed path canonically as continuous vector segments.
    pub fn register_route(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        material_id: u8,
        width_nm: i64,
    ) {
        self.register_route_with_z_materials(
            net_id,
            waypoints,
            material_id,
            width_nm,
            None::<fn(i64) -> Option<u8>>,
        )
    }

    /// Register a route with Z-aware material resolution (v0.1.9.1)
    pub fn register_route_with_z_materials<F>(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        default_material_id: u8,
        width_nm: i64,
        z_to_material: Option<F>,
    ) where
        F: Fn(i64) -> Option<u8>,
    {
        if waypoints.len() < 2 {
            return;
        }

        let deduped: Vec<crate::geometry::Point3D> = waypoints
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, p)| *i == 0 || *p != waypoints[i - 1])
            .map(|(_, p)| p)
            .collect();

        if deduped.len() < 2 {
            return;
        }

        let segments: Vec<TraceSegment> = deduped
            .windows(2)
            .filter(|w| w[0] != w[1])
            .map(|w| {
                let start = w[0];
                let end = w[1];

                let seg_material_id = if start.z == end.z {
                    if let Some(ref resolver) = z_to_material {
                        resolver(start.z).unwrap_or(default_material_id)
                    } else {
                        default_material_id
                    }
                } else {
                    default_material_id
                };

                TraceSegment::new(start, end, width_nm, seg_material_id)
            })
            .collect();

        if segments.is_empty() {
            return;
        }

        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
    }

    /// Register pre-built trace segments directly (for lockfile loading).
    pub fn register_trace_segments(&mut self, net_id: NetId, segments: Vec<TraceSegment>) {
        if segments.is_empty() {
            return;
        }

        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
    }

    /// Get all canonically registered route segments across all nets.
    pub fn get_all_routes(&self) -> &[(NetId, Vec<TraceSegment>)] {
        &self.routed_segments
    }

    /// Clear registered route segments for a specific net.
    pub fn clear_routes_for_net(&mut self, net_id: NetId) {
        self.routed_segments.retain(|(id, _)| *id != net_id);
    }

    /// Register a single point as occupied by a net (for polygon rasterization).
    pub fn occupy_point(
        &mut self,
        point: crate::geometry::Point3D,
        net_id: NetId,
        material: crate::geometry_router::substrate_types::MaterialId,
    ) {
        let segment = TraceSegment::new(point, point, 0, material);
        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.push(segment);
        } else {
            self.routed_segments.push((net_id, vec![segment]));
        }
    }

    /// **v0.2.0: DATABASE-DRIVEN SYNCHRONIZATION**
    ///
    /// Synchronize routed_segments from the hierarchical routing database.
    /// This is the ONLY way routed_segments should be populated in hierarchical designs.
    ///
    /// # Architecture
    ///
    /// - `routing_database` is the single source of truth
    /// - `entity_graph.routed_segments` is a read-only view for obstacle queries
    /// - This function rebuilds the view from the database
    ///
    /// # When to Call
    ///
    /// - After hierarchical flattening completes (all child routes registered)
    /// - Before parent-level routing begins (so router sees child routes as same-net)
    /// - After loading from lockfile (to restore routing state)
    ///
    /// # Guarantees
    ///
    /// - No hardcoded defaults
    /// - No fallbacks
    /// - No split-brain between database and entity_graph
    /// - Child routes are treated as same-net obstacles (not hard obstacles)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // After flattening all child instances
    /// space.entity_graph.sync_from_routing_database(&space.routing_database);
    ///
    /// // Now parent-level routing can see child routes as same-net
    /// route_parent_interconnects(space)?;
    /// ```
    pub fn sync_from_routing_database(
        &mut self,
        routing_database: &crate::routing_database::HierarchicalRoutingDatabase,
        routing_layer_db: &crate::routing_layer_database::RoutingLayerDatabase,
    ) {
        eprintln!("[ENTITY_GRAPH SYNC] Synchronizing routed_segments from routing database");
        eprintln!(
            "[ENTITY_GRAPH SYNC]   Before sync: {} route groups",
            self.routed_segments.len()
        );

        // CLEAR existing routed_segments - database is source of truth
        self.routed_segments.clear();

        // REBUILD from database export with proper per-segment material lookup
        // **v0.2.2**: Use direct layer lineage lookup instead of reverse Z-coordinate guessing
        let database_routes =
            routing_database.export_as_routed_segments_with_lineage(routing_layer_db);

        eprintln!(
            "[ENTITY_GRAPH SYNC]   Database provided {} route groups",
            database_routes.len()
        );

        for (net_id, segments) in database_routes {
            if !segments.is_empty() {
                eprintln!(
                    "[ENTITY_GRAPH SYNC]   Syncing net {:?}: {} segments",
                    net_id,
                    segments.len()
                );
                self.routed_segments.push((net_id, segments));
            }
        }

        eprintln!(
            "[ENTITY_GRAPH SYNC]   After sync: {} route groups",
            self.routed_segments.len()
        );
        eprintln!("[ENTITY_GRAPH SYNC]   Synchronization complete ✓");
    }

    /// **v0.2.1: SPATIAL INDEX POPULATION**
    ///
    /// Index all routed segments into the spatial index for DRC and collision detection.
    /// This method encapsulates the logic for creating IndexedSegment objects and avoids
    /// borrow checker issues by working with owned data.
    ///
    /// # Arguments
    ///
    /// * `layer_resolver` - Closure that maps (z_nm) -> Result<(layer_name, thickness_nm), Error>
    /// * `material_validator` - Closure that validates material_id exists
    ///
    /// # Returns
    ///
    /// Result with the number of segments indexed, or an error message
    ///
    /// # Example
    ///
    /// ```ignore
    /// space.entity_graph.index_routes_into_spatial(
    ///     |z| stackup_manager.get_layer_index_at_z(z)
    ///         .and_then(|idx| {
    ///             let name = stackup_manager.ordered_layers()[idx].clone();
    ///             let thickness = stackup_manager.get_thickness_for_layer_index(idx)?;
    ///             Ok((name, thickness))
    ///         }),
    ///     |mat_id| space.material_registry.get_material(mat_id).is_some(),
    /// )?;
    /// ```
    pub fn index_routes_into_spatial<F, G, E>(
        &mut self,
        mut layer_resolver: F,
        material_validator: G,
    ) -> Result<usize, E>
    where
        F: FnMut(i64) -> Result<(String, i64), E>,
        G: Fn(u8) -> bool,
    {
        // Collect route data first to avoid borrow conflicts
        let route_data: Vec<(usize, NetId, Vec<TraceSegment>)> = self
            .routed_segments
            .iter()
            .enumerate()
            .map(|(net_idx, (net_id, segments))| (net_idx, *net_id, segments.clone()))
            .collect();

        let mut indexed_count = 0;

        for (net_idx, net_id, segments) in route_data {
            eprintln!(
                "[COMPILATION DEBUG] Route net {} has {} segments",
                net_id.raw(),
                segments.len()
            );

            for (seg_idx, segment) in segments.iter().enumerate() {
                // Validate material
                if !material_validator(segment.material_id) {
                    return Err(format!(
                        "Route segment {}/{} on net {} references unknown material_id {}",
                        net_idx,
                        seg_idx,
                        net_id.raw(),
                        segment.material_id
                    ))
                    .map_err(|s| panic!("{}", s))
                    .unwrap();
                }

                // Resolve layer information
                let (layer_name, thickness_nm) = layer_resolver(segment.start.z)?;

                let indexed_segment = hwc_physics::spatial_index::IndexedSegment::new(
                    hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx,
                        seg_idx,
                    },
                    seg_idx,
                    net_id,
                    &segment,
                    segment.start.z,
                    thickness_nm,
                );

                eprintln!(
                    "[COMPILATION DEBUG] Indexing route segment: net={}, seg={}, layer={}, Z={}, thickness={}nm",
                    net_id.raw(), seg_idx, layer_name, segment.start.z, thickness_nm
                );

                self.spatial.insert(indexed_segment);
                indexed_count += 1;
            }
        }

        Ok(indexed_count)
    }
}
