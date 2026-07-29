//! Main routing engine orchestration

use super::types::{GeometryRouter, RouteSpaceRequest};
use crate::geometry::Point3D;
use crate::geometry_router::bounding_box_tracker::BoundingBoxTracker;
use crate::geometry_router::neighbor_generation::GridBounds;
use crate::geometry_router::pathfinding::CostComposer;
use crate::geometry_router::types::{RouteResult, RoutedNet, RoutingError};
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Primary entrypoint for space-level routing with adaptive mode selection.
    pub fn route_space(&mut self, req: RouteSpaceRequest) -> Result<RouteResult, RoutingError> {
        let RouteSpaceRequest {
            grid_bbox,
            nets,
            explicit_segments,
            obstacle_bboxes,
            substrate_layers,
            net_frequencies,
            net_trace_widths,
            net_normals,
            net_escape_stubs,
            net_layer_targets,
        } = req;

        if let Some(sl) = substrate_layers {
            self.substrate_layers = Some(sl.to_vec());
        }
        self.net_frequencies = net_frequencies.clone();
        self.net_trace_widths = net_trace_widths.clone();
        
        // v0.1.9: Store per-net normals and escape stubs for perpendicular escape routing
        if let Some(normals) = net_normals {
            self.net_normals = normals.clone();
        }
        if let Some(escape_stubs) = net_escape_stubs {
            self.net_escape_stubs = escape_stubs.clone();
        }
        
        // v0.2.0: Store per-net layer targets for explicit layer routing
        if let Some(layer_targets) = net_layer_targets {
            self.net_layer_targets = layer_targets.clone();
            for (net_id, target_z) in layer_targets {
                eprintln!("[ROUTER ENGINE] Net {} has explicit layer target at Z={}nm", 
                    net_id.raw(), target_z);
            }
        }

        self.build_entity_graph();

        let track_pitch = self.resolution_nm;
        let max_clearance = self
            .constraints
            .fabrication
            .as_ref()
            .expect("BUG: Fabrication constraints required for routing partition grid. \
                     Ensure the profile defines 'trace.min_spacing'.")
            .min_trace_spacing_nm;
        let partition = crate::geometry_router::partition::PartitionGrid::new(
            *grid_bbox,
            10_000_000,
            10_000_000,
            track_pitch,
            max_clearance,
        );
        self.partition_grid = Some(partition);

        let mut result = if let Some(segments) = explicit_segments {
            self.route_all_nets_explicit_global(segments)?
        } else {
            RouteResult::new()
        };

        let width = grid_bbox.max.x - grid_bbox.min.x;
        let height = grid_bbox.max.y - grid_bbox.min.y;
        let area_nm2 = width * height;
        let net_count = nets.len();

        if area_nm2 < self.config.area_threshold_nm2 && net_count < self.config.net_count_threshold
        {
            let steiner_result = self.route_all_nets_steiner(
                nets,
                obstacle_bboxes,
                substrate_layers,
                net_frequencies,
            )?;
            result.merge(steiner_result);
            self.apply_refinement_pipeline(&mut result);
            Ok(result)
        } else {
            let hierarchical_result = self.route_hierarchical(
                grid_bbox,
                nets,
                obstacle_bboxes,
                substrate_layers,
                net_frequencies,
            )?;
            result.merge(hierarchical_result);
            self.apply_refinement_pipeline(&mut result);
            Ok(result)
        }
    }

    /// Pass-Through routing: routes all nets in a single pass over the entire board.
    pub fn route_all_nets_steiner(
        &mut self,
        nets: &FxHashMap<crate::netlist::NetId, Vec<Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        _substrate_layers: Option<&[crate::geometry_router::substrate_types::SubstrateLayer]>,
        _net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<RouteResult, RoutingError> {
        self.route_all_nets_steiner_global(nets)
    }

    /// Hierarchical routing: partition into G-Cells, global route, then parallel detailed routing.
    fn route_hierarchical(
        &mut self,
        grid_bbox: &crate::geometry::BoundingBox,
        nets: &FxHashMap<crate::netlist::NetId, Vec<Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        substrate_layers: Option<&[crate::geometry_router::substrate_types::SubstrateLayer]>,
        net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<RouteResult, RoutingError> {
        let cell_size_nm = 10_000_000;
        let gcell_grid = crate::geometry_router::router::global_router::GCellGrid::partition(
            grid_bbox,
            cell_size_nm,
        );

        let mut cross_cell_nets = FxHashMap::default();
        let mut intra_cell_nets: Vec<FxHashMap<crate::netlist::NetId, Vec<Point3D>>> =
            vec![FxHashMap::default(); gcell_grid.cells.len()];

        for (net_id, pins) in nets {
            let mut cell_indices = rustc_hash::FxHashSet::default();
            for p in pins {
                if let Some(idx) = gcell_grid.get_cell_index_at(p.x, p.y) {
                    cell_indices.insert(idx);
                }
            }

            if cell_indices.len() > 1 {
                cross_cell_nets.insert(*net_id, pins.clone());
            } else if let Some(&idx) = cell_indices.iter().next() {
                intra_cell_nets[idx].insert(*net_id, pins.clone());
            }
        }

        let mut final_result = RouteResult::new();

        if !cross_cell_nets.is_empty() {
            let mut sorted_cross: Vec<_> = cross_cell_nets.iter().collect();
            sorted_cross.sort_by_key(|(id, _)| id.0);

            let cross_results: Vec<(crate::netlist::NetId, Result<RoutedNet, RoutingError>)> =
                std::thread::scope(|s| {
                    let mut handles = Vec::new();

                    for &(&net_id, pins) in &sorted_cross {
                        let entity_graph_clone = self.entity_graph.clone();
                        let bounds = self.bounds;
                        let constraints = self.constraints.clone();
                        let layer_directions = self.layer_directions.clone();
                        let resolution_nm = self.resolution_nm;
                        let material_registry = self.material_registry.clone();
                        let copper_pours = self.copper_pours.clone();
                        let bounding_box_tracker = self.bounding_box_tracker.clone();
                        let config = self.config.clone();
                        let substrate_layers = self.substrate_layers.clone();
                        let net_frequencies = self.net_frequencies.clone();
                        let route_net_policies = self.route_net_policies.clone();
                        let routing_material_id = self.routing_material_id;
                        let trace_width_nm = self.trace_width_nm;
                        let net_trace_widths = self.net_trace_widths.clone();

                        let handle = s.spawn(move || {
                            if pins.len() < 2 {
                                return (
                                    net_id,
                                    Ok(RoutedNet {
                                        net_id,
                                        paths: vec![vec![pins[0], pins[0]]],
                                        vias: Vec::new(),
                                    }),
                                );
                            }

                            let mut isolated_entity_graph =
                                crate::geometry_router::EntityGraph::new();
                            isolated_entity_graph.copy_metadata_from(&entity_graph_clone);

                            let mut isolated = GeometryRouter {
                                bounds,
                                constraints,
                                layer_directions,
                                resolution_nm,
                                material_registry,
                                entity_graph: isolated_entity_graph,
                                vias: Vec::new(),
                                copper_pours,
                                bounding_box_tracker,
                                config,
                                substrate_layers,
                                net_frequencies,
                                partition_grid: None,
                                query_store: None,
                                route_net_policies,
                                routing_material_id,
                                trace_width_nm,
                                net_trace_widths,
                                net_normals: FxHashMap::default(),
                                net_escape_stubs: FxHashMap::default(),
                                cost_composer: CostComposer::default(),
                                intent_composers: FxHashMap::default(),
                                net_layer_targets: FxHashMap::default(),
                            };

                            let result = isolated.decompose_net_steiner(net_id, pins);
                            (net_id, result)
                        });
                        handles.push(handle);
                    }

                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });

            for (net_id, result) in cross_results {
                let routed = result?;

                for segment in &routed.paths {
                    let layer_z_positions = self.config.layer_z_positions.clone();
                    let layer_materials = self.config.layer_materials.clone();
                    let z_to_material = move |z: i64| -> Option<u8> {
                        for i in 0..layer_z_positions.len() {
                            let z_start = layer_z_positions[i];
                            let z_end = if i + 1 < layer_z_positions.len() {
                                layer_z_positions[i + 1]
                            } else {
                                i64::MAX
                            };
                            if z >= z_start && z < z_end {
                                return layer_materials.get(i).copied();
                            }
                        }
                        None
                    };

                    self.entity_graph.register_route_with_z_materials(
                        net_id,
                        segment,
                        self.routing_material_id,
                        self.trace_width_nm,
                        Some(z_to_material),
                    );
                }

                final_result.paths.insert(net_id, routed.paths);
                final_result.vias.extend(routed.vias);
            }
        }

        if !intra_cell_nets.is_empty() {
            if self.query_store.is_some() {
                let file_id = 0u64;

                for cell in &gcell_grid.cells {
                    let cell_nets = match intra_cell_nets.get(cell.id) {
                        Some(nets) => nets,
                        None => continue,
                    };

                    let gcell_id = cell.id as u32;
                    let query_id = crate::geometry_router::query_engine::make_query_id(
                        crate::geometry_router::query_engine::QueryType::RouteGcell,
                        file_id,
                        &[gcell_id as u64],
                    );

                    let is_cached = self
                        .query_store
                        .as_ref()
                        .unwrap()
                        .get_result(query_id)
                        .is_some();

                    if is_cached {
                        continue;
                    }

                    let cell_bbox = &cell.bbox;
                    let mut cell_router = GeometryRouter {
                        bounds: GridBounds::new(
                            cell_bbox.max.x - cell_bbox.min.x,
                            cell_bbox.max.y - cell_bbox.min.y,
                            cell_bbox.max.z - cell_bbox.min.z,
                        ),
                        constraints: self.constraints.clone(),
                        layer_directions: self.layer_directions.clone(),
                        resolution_nm: self.resolution_nm,
                        material_registry: self.material_registry.clone(),
                        entity_graph: crate::geometry_router::EntityGraph::new(),
                        vias: Vec::new(),
                        copper_pours: Vec::new(),
                        bounding_box_tracker: BoundingBoxTracker::new(),
                        config: self.config.clone(),
                        substrate_layers: self.substrate_layers.clone(),
                        net_frequencies: self.net_frequencies.clone(),
                        partition_grid: None,
                        query_store: None,
                        route_net_policies: self.route_net_policies.clone(),
                        routing_material_id: self.routing_material_id,
                        trace_width_nm: self.trace_width_nm,
                        net_trace_widths: self.net_trace_widths.clone(),
                        net_normals: FxHashMap::default(),
                        net_escape_stubs: FxHashMap::default(),
                        cost_composer: CostComposer::default(),
                        intent_composers: FxHashMap::default(),
                        net_layer_targets: FxHashMap::default(),
                    };

                    let local_nets: FxHashMap<
                        crate::netlist::NetId,
                        Vec<crate::geometry::Point3D>,
                    > = cell_nets
                        .iter()
                        .map(|(&net_id, pins)| {
                            let local_pins: Vec<_> = pins
                                .iter()
                                .map(|pin| {
                                    crate::geometry::Point3D::new(
                                        pin.x - cell_bbox.min.x,
                                        pin.y - cell_bbox.min.y,
                                        pin.z - cell_bbox.min.z,
                                    )
                                })
                                .collect();
                            (net_id, local_pins)
                        })
                        .collect();

                    cell_router.substrate_layers = substrate_layers.map(|sl| sl.to_vec());
                    cell_router.net_frequencies = net_frequencies.clone();
                    cell_router
                        .entity_graph
                        .copy_metadata_from(&self.entity_graph);

                    match cell_router.route_all_nets_steiner_global(&local_nets) {
                        Ok(local_result) => {
                            let mut cell_result = RouteResult::new();
                            for (net_id, local_paths) in &local_result.paths {
                                let global_paths: Vec<Vec<_>> = local_paths
                                    .iter()
                                    .map(|segment| {
                                        segment
                                            .iter()
                                            .map(|pt| {
                                                crate::geometry::Point3D::new(
                                                    pt.x + cell_bbox.min.x,
                                                    pt.y + cell_bbox.min.y,
                                                    pt.z + cell_bbox.min.z,
                                                )
                                            })
                                            .collect()
                                    })
                                    .collect();
                                cell_result.paths.insert(*net_id, global_paths);
                            }
                            cell_result.vias.extend(local_result.vias);

                            let segment_count: usize =
                                cell_result.paths.values().map(|segs| segs.len()).sum();
                            let hash_input = [
                                file_id.to_le_bytes(),
                                (gcell_id as u64).to_le_bytes(),
                                (segment_count as u64).to_le_bytes(),
                            ]
                            .concat();
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            use std::hash::{Hash, Hasher};
                            hash_input.hash(&mut hasher);
                            let hash_val = hasher.finish();
                            let mut hash_bytes = [0u8; 32];
                            hash_bytes[..8].copy_from_slice(&hash_val.to_le_bytes());

                            let route_result = crate::geometry_router::query_engine::RouteResult {
                                file_id,
                                gcell_id,
                                segment_count,
                                hash: hash_bytes,
                            };
                            self.query_store
                                .as_mut()
                                .unwrap()
                                .execute_query(query_id, || {
                                    crate::geometry_router::query_engine::QueryResult::RouteGcell(
                                        route_result,
                                    )
                                });

                            final_result.merge(cell_result);
                        }
                        Err(e) => return Err(e),
                    }
                }
            } else {
                let entity_graph_ref = &self.entity_graph;
                let intra_results: Vec<Result<RouteResult, RoutingError>> =
                    std::thread::scope(|s| {
                        let mut handles = Vec::new();

                        for cell in &gcell_grid.cells {
                            let cell_nets = match intra_cell_nets.get(cell.id) {
                                Some(nets) => nets,
                                None => {
                                    handles.push(s.spawn(|| Ok(RouteResult::new())));
                                    continue;
                                }
                            };

                            let cell_bbox = cell.bbox;
                            let local_nets: FxHashMap<
                                crate::netlist::NetId,
                                Vec<crate::geometry::Point3D>,
                            > = cell_nets
                                .iter()
                                .map(|(&net_id, pins)| {
                                    let local_pins: Vec<_> = pins
                                        .iter()
                                        .map(|pin| {
                                            crate::geometry::Point3D::new(
                                                pin.x - cell_bbox.min.x,
                                                pin.y - cell_bbox.min.y,
                                                pin.z - cell_bbox.min.z,
                                            )
                                        })
                                        .collect();
                                    (net_id, local_pins)
                                })
                                .collect();

                            let constraints = self.constraints.clone();
                            let layer_directions = self.layer_directions.clone();
                            let resolution_nm = self.resolution_nm;
                            let material_registry = self.material_registry.clone();
                            let copper_pours = self.copper_pours.clone();
                            let config = self.config.clone();
                            let substrate_layers_vec = substrate_layers.map(|sl| sl.to_vec());
                            let net_frequencies_clone = net_frequencies.clone();
                            let self_net_frequencies = self.net_frequencies.clone();
                            let route_net_policies = self.route_net_policies.clone();
                            let routing_material_id = self.routing_material_id;
                            let trace_width_nm = self.trace_width_nm;
                            let net_trace_widths = self.net_trace_widths.clone();

                            let handle = s.spawn(move || {
                                let mut cell_router = GeometryRouter {
                                    bounds: GridBounds::new(
                                        cell_bbox.max.x - cell_bbox.min.x,
                                        cell_bbox.max.y - cell_bbox.min.y,
                                        cell_bbox.max.z - cell_bbox.min.z,
                                    ),
                                    constraints,
                                    layer_directions,
                                    resolution_nm,
                                    material_registry,
                                    entity_graph: crate::geometry_router::EntityGraph::new(),
                                    vias: Vec::new(),
                                    copper_pours,
                                    bounding_box_tracker: BoundingBoxTracker::new(),
                                    config,
                                    substrate_layers: substrate_layers_vec.clone(),
                                    net_frequencies: self_net_frequencies,
                                    partition_grid: None,
                                    query_store: None,
                                    route_net_policies,
                                    routing_material_id,
                                    trace_width_nm,
                                    net_trace_widths,
                                    net_normals: FxHashMap::default(),
                                    net_escape_stubs: FxHashMap::default(),
                                    cost_composer: CostComposer::default(),
                                    intent_composers: FxHashMap::default(),
                                    net_layer_targets: FxHashMap::default(),
                                };

                                cell_router.net_frequencies = net_frequencies_clone;
                                cell_router
                                    .entity_graph
                                    .copy_metadata_from(entity_graph_ref);

                                let mut cell_result = RouteResult::new();
                                match cell_router.route_all_nets_steiner_global(&local_nets) {
                                    Ok(local_result) => {
                                        for (net_id, local_paths) in &local_result.paths {
                                            let global_paths: Vec<Vec<_>> = local_paths
                                                .iter()
                                                .map(|segment| {
                                                    segment
                                                        .iter()
                                                        .map(|pt| {
                                                            crate::geometry::Point3D::new(
                                                                pt.x + cell_bbox.min.x,
                                                                pt.y + cell_bbox.min.y,
                                                                pt.z + cell_bbox.min.z,
                                                            )
                                                        })
                                                        .collect()
                                                })
                                                .collect();
                                            cell_result.paths.insert(*net_id, global_paths);
                                        }
                                        cell_result.vias.extend(local_result.vias);
                                    }
                                    Err(e) => return Err(e),
                                }

                                Ok(cell_result)
                            });
                            handles.push(handle);
                        }

                        handles.into_iter().map(|h| h.join().unwrap()).collect()
                    });

                for res in intra_results {
                    final_result.merge(res?);
                }
            }
        }

        Ok(final_result)
    }
}
