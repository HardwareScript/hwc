use super::builder::RoutingData;
use super::config::AutoRouter;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::RouteResult;
use hwc_engine::netlist::NetId;

impl<'a> AutoRouter<'a> {
    pub(crate) fn post_process_routes(
        &mut self,
        mut result: RouteResult,
        data: &RoutingData,
    ) -> Result<(), IrError> {
        if !self.config.route_net_policies.is_empty() {
            result = self.inject_meanders(result, data)?;
        }

        let trace_thickness_nm = self.resolve_trace_thickness(&result)?;
        let trace_width = self.require_trace_width()?;

        for (net_id_raw, segments) in &result.paths {
            let actual_net_id = if !self.config.auto_routes.is_empty() {
                NetId::new(net_id_raw.raw() % 10000)
            } else {
                *net_id_raw
            };

            let net_name = data
                .net_id_to_name
                .get(net_id_raw)
                .cloned()
                .unwrap_or_else(|| CompactString::from(format!("net_{}", actual_net_id.raw())));

            // **v0.2.0 FIX: Process all path segments for this net and merge into single route**
            // The router may return multiple disconnected path segments. We process each
            // segment independently but only call register_analytic_route once with a merged
            // path to avoid duplicate parent route registration.
            
            // **BUG FIX v0.2.1: Process route segments independently, then combine**
            // Previously, all route segments were concatenated into a single waypoint array,
            // which caused manhattan_path_to_segments to incorrectly delete valid routes due to
            // false collinearity detection between unrelated segments.
            //
            // The fix: Process each route statement separately to preserve route boundaries,
            // then combine all processed segments into a single AnalyticTrace registration.
            // This prevents both the concatenation bug AND duplicate parent route errors.
            
            let mut all_segments: Vec<hwc_engine::LineSegment> = Vec::new();
            let mut first_thickness = trace_thickness_nm;
            let mut route_count = 0;

            for path in segments {
                if path.len() < 2 {
                    continue;
                }
                
                eprintln!("[POST_PROCESS DEBUG] Net {:?} path BEFORE miter (len={}):", net_id_raw, path.len());
                for (i, p) in path.iter().enumerate().take(5) {
                    eprintln!("[POST_PROCESS DEBUG]   [{}]: ({},{},{})", i, p.x, p.y, p.z);
                }
                
                let miter_engine = hwc_engine::MiterEngine::new(trace_width);
                
                // **v0.2.0: Context-aware mitering** - query the space for via locations
                // and pass as context to preserve via landing pad connections
                let mitered_path = miter_engine.apply_miter_pass_with_context(
                    path,
                    &*self.space as &dyn hwc_engine::geometry_router::miter_pass::MiterContext,
                    Some(*net_id_raw),
                );
                
                eprintln!("[POST_PROCESS DEBUG] Net {:?} path AFTER miter (len={}):", net_id_raw, mitered_path.len());
                for (i, p) in mitered_path.iter().enumerate().take(5) {
                    eprintln!("[POST_PROCESS DEBUG]   [{}]: ({},{},{})", i, p.x, p.y, p.z);
                }
                
                let (refined_path, actual_thickness) =
                    self.refine_path_z(mitered_path, trace_thickness_nm)?;

                let mut final_path = refined_path;
                
                // STRUCTURAL FIX: Only add vertical transitions if the path doesn't already have them
                // The new routing engine (v0.2.0) already includes vertical transitions in the path
                let has_z_transitions = final_path.windows(2).any(|w| w[0].z != w[1].z);
                if !has_z_transitions {
                    eprintln!("[POST_PROCESS] Path is planar - adding vertical transitions");
                    self.add_vertical_transitions(&mut final_path, &net_name, data);
                } else {
                    eprintln!("[POST_PROCESS] Path already has Z transitions - skipping add_vertical_transitions");
                }

                // Store the first thickness value
                if route_count == 0 {
                    first_thickness = actual_thickness;
                }

                // Convert path to segments independently (avoiding concatenation bug)
                if final_path.len() >= 2 {
                    let min_seg_len_nm = crate::ir::routing::helpers::require_min_segment_length_nm(self.profile)?;
                    
                    let has_z_transitions = final_path.windows(2).any(|w| w[0].z != w[1].z);
                    let has_diagonal_segments = final_path.windows(2).any(|w| {
                        let dx = (w[1].x - w[0].x).abs();
                        let dy = (w[1].y - w[0].y).abs();
                        let dz = (w[1].z - w[0].z).abs();
                        (dx > 0 && dy > 0) || (dx > 0 && dz > 0) || (dy > 0 && dz > 0)
                    });
                    
                    let route_segments = if has_z_transitions || has_diagonal_segments {
                        let mut segs = Vec::new();
                        for i in 0..final_path.len() - 1 {
                            segs.push(hwc_engine::LineSegment::new(final_path[i], final_path[i + 1]));
                        }
                        segs
                    } else {
                        crate::ir::routing::helpers::manhattan_path_to_segments(&final_path, min_seg_len_nm)
                    };
                    
                    all_segments.extend(route_segments);
                    route_count += 1;
                }
            }

            // Register all segments as a single parent route
            if !all_segments.is_empty() {
                let declared_width = data
                    .net_declared_widths
                    .get::<str>(net_name.as_ref())
                    .copied();
                let current_ma = self.resolve_net_current(&net_name, data)?;

                // Determine the routing layer for this net
                let routing_layer_name = data
                    .net_layer_names_by_id
                    .get(&actual_net_id)
                    .ok_or_else(|| IrError::RoutingError(format!(
                        "Could not determine routing layer for net '{}' - no layer name recorded",
                        net_name
                    )))?;

                // Create a single AnalyticTrace with all segments
                self.register_analytic_route_from_segments(
                    actual_net_id,
                    &net_name,
                    all_segments,
                    routing_layer_name,
                    first_thickness,
                    declared_width,
                    current_ma,
                )?;
            }
        }

        self.space.entity_graph.commit_route();
        // v0.2.0: Routes are now registered directly in the routing database.
        // No re-registration needed.

        self.run_legalization()?;
        self.configure_entity_graph_spatial()?;
        self.rebuild_analytic_routes()?;

        Ok(())
    }

    fn inject_meanders(
        &mut self,
        result: RouteResult,
        data: &RoutingData,
    ) -> Result<RouteResult, IrError> {
        let trace_width = self.require_trace_width()?;
        let min_clearance = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Meander injection requires spacing constraints.".into(),
                hint: "Add 'trace:' block.".into(),
            })?;

        let injector = crate::ir::meander_injection::MeanderInjector::new(
            &self.config.route_net_policies,
            &data.obstacle_bboxes,
            trace_width,
            min_clearance,
        );
        let result = injector.inject(result);

        // **v0.2.0: DO NOT CLEAR entity_graph routes here!**
        // Child routes from hierarchical flattening are in entity_graph.routed_segments.
        // Clearing them would break same-net obstacle detection for subsequent routes.
        //
        // Architecture principle: routing_database is the source of truth.
        // entity_graph.routed_segments is a read-only view synced from the database.
        // AutoRouter already registers routes in routing_database via register_autorouter_route().
        //
        // The old pattern was:
        //   1. Clear entity_graph for this net
        //   2. Re-register new routes in entity_graph
        //
        // The new pattern is:
        //   1. Routes are registered in routing_database during routing
        //   2. entity_graph stays synchronized (child + parent routes coexist)
        //
        // No action needed here - routes are already in the database.
        
        // NOTE: The loop below is now a NO-OP since we don't clear or register.
        // Keeping it commented for reference during transition.
        // for (&net_id, mutated_paths) in &result.paths {
        //     self.space.entity_graph.clear_routes_for_net(net_id);  // REMOVED
        //     for path in mutated_paths {
        //         self.space.entity_graph.register_route(  // REMOVED
        //             net_id,
        //             path,
        //             routing_copper_id,
        //             trace_width,
        //         );
        //     }
        // }
        
        Ok(result)
    }

    fn resolve_trace_thickness(&self, result: &RouteResult) -> Result<i64, IrError> {
        let default_thickness = self.space.resolution_nm;
        let sample_z = result
            .paths
            .values()
            .next()
            .and_then(|s| s.first())
            .and_then(|p| p.first())
            .map(|p| p.z)
            .unwrap_or(0);
        self.stackup_manager
            .get_layer_index_at_z(sample_z)
            .map(|idx| self.stackup_manager.get_thickness_for_layer_index(idx))
            .unwrap_or(Ok(default_thickness))
    }

    fn require_trace_width(&self) -> Result<i64, IrError> {
        self.space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Missing trace width constraint.".into(),
                hint: "Add 'trace:' block.".into(),
            })
    }

    fn refine_path_z(
        &self,
        mut path: Vec<Point3D>,
        default_thickness: i64,
    ) -> Result<(Vec<Point3D>, i64), IrError> {
        // STRUCTURAL FIX: Check if path already has Z transitions BEFORE refining
        let has_z_transitions = path.windows(2).any(|w| w[0].z != w[1].z);
        
        if has_z_transitions {
            
            // Path already has vertical transitions from the new router
            // Don't flatten the Z coordinates - just determine the thickness
            let first_z = path.first().map(|p| p.z).unwrap_or(0);
            let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
            let actual_thickness = if let Some(layer_idx) = first_layer {
                self.stackup_manager.get_thickness_for_layer_index(layer_idx)?
            } else {
                default_thickness
            };
            return Ok((path, actual_thickness));
        }
        
       
        
        let first_z = path.first().map(|p| p.z).unwrap_or(0);
        let last_z = path.last().map(|p| p.z).unwrap_or(0);
        let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
        let last_layer = self.stackup_manager.get_layer_index_at_z(last_z);

        let mut actual_thickness = default_thickness;
        let target_z = match (first_layer, last_layer) {
            (Some(a), Some(b)) if a == b => {
                actual_thickness = self.stackup_manager.get_thickness_for_layer_index(a)?;
                Some((first_z + last_z) / 2)
            }
            (Some(a), _) => {
                actual_thickness = self.stackup_manager.get_thickness_for_layer_index(a)?;
                Some(first_z)
            }
            _ => None,
        };

        if let Some(z) = target_z {
            for point in path.iter_mut() {
                point.z = z;
            }
        } else {
            for point in path.iter_mut() {
                if let Some(idx) = self.stackup_manager.get_layer_index_at_z(point.z) {
                    point.z = self.stackup_manager.get_z_start_nm_for_layer_index(idx)?;
                }
            }
        }
        Ok((path, actual_thickness))
    }

    fn add_vertical_transitions(
        &self,
        path: &mut Vec<Point3D>,
        net_name: &CompactString,
        data: &RoutingData,
    ) {
        if let Some(&target_z) = data.net_layer_targets.get::<str>(net_name.as_ref()) {
            let original_pin_z = path.first().map(|p| p.z).unwrap_or(0);
            let pin_layer = self.stackup_manager.get_layer_index_at_z(original_pin_z);
            let target_layer = self.stackup_manager.get_layer_index_at_z(target_z);

            if match (pin_layer, target_layer) {
                (Some(p), Some(t)) => p != t,
                _ => true,
            } && original_pin_z != target_z
            {
                let start = *path.first().unwrap();
                path.insert(0, Point3D::new(start.x, start.y, original_pin_z));
                path.insert(1, Point3D::new(start.x, start.y, target_z));

                let end = *path.last().unwrap();
                path.push(Point3D::new(end.x, end.y, target_z));
                path.push(Point3D::new(end.x, end.y, original_pin_z));
            }
        }
    }

    fn resolve_net_current(
        &self,
        net_name: &CompactString,
        data: &RoutingData,
    ) -> Result<f64, IrError> {
        data.net_currents_ma
            .get::<str>(net_name.as_ref())
            .copied()
            .ok_or_else(|| {
                if self.profile.as_ref().is_some_and(|p| p.is_asic()) {
                    IrError::MissingAsicConstraint {
                        message: format!("Net '{}' missing current declaration.", net_name),
                        hint: "Add current limit.".into(),
                    }
                } else {
                    IrError::MissingAsicConstraint {
                        message: "Internal error".into(),
                        hint: "".into(),
                    }
                }
            })
            .or_else(|e| {
                if self.profile.as_ref().is_some_and(|p| p.is_asic()) {
                    Err(e)
                } else {
                    Ok(0.0)
                }
            })
    }

    fn run_legalization(&mut self) -> Result<(), IrError> {
        // **v0.2.0: Legalization should NOT modify entity_graph directly**
        // entity_graph.routed_segments contains child routes that must be preserved.
        // Legalization operates on PARENT routes only, which are in routing_database.
        //
        // TODO: This legalization code needs refactoring to:
        // 1. Get parent routes from routing_database (not entity_graph)
        // 2. Legalize them
        // 3. Update them in routing_database
        // 4. Re-sync entity_graph from database
        //
        // For now, we'll skip legalization in hierarchical designs to avoid data corruption.
        eprintln!("[LEGALIZATION] Skipping post-routing legalization (hierarchical design - needs refactor)");
        
        // Original legalization code (DISABLED to prevent clearing child routes):
        // let legalizer = hwc_engine::geometry_router::Legalizer::new(min_clearance);
        // let all_routes = self.space.entity_graph.get_all_routes();
        // ... (rest of legalization code that clears and re-registers)
        
        Ok(())
    }

    fn configure_entity_graph_spatial(&mut self) -> Result<(), IrError> {
        if let Some(_profile) = self.profile {
            let profile_layers = self.stackup_manager.ordered_layers();
            if !profile_layers.is_empty() {
                let mut z_ranges = Vec::with_capacity(profile_layers.len());
                for i in 0..profile_layers.len() {
                    let z_min = self
                        .stackup_manager
                        .get_layer_start_z(&profile_layers[i])
                        .unwrap_or(0);
                    let z_max = if i + 1 < profile_layers.len() {
                        self.stackup_manager
                            .get_layer_start_z(&profile_layers[i + 1])
                            .unwrap_or(self.space.dimensions.depth_nm)
                    } else {
                        self.space.dimensions.depth_nm
                    };
                    z_ranges.push((z_min, z_max));
                }
                self.space
                    .entity_graph
                    .set_spatial_layer_z_ranges(&z_ranges);
            }
        }
        Ok(())
    }

    fn rebuild_analytic_routes(&mut self) -> Result<(), IrError> {
        // v0.2.0: Build analytic_routes from the routing database (single source of truth)
        self.space.sync_analytic_routes_from_database();

        // Validate routing database consistency
        self.space.routing_database.validate()
            .map_err(|errors| IrError::RoutingError(
                format!("Routing database validation failed:\n{}", errors.join("\n"))
            ))?;

        Ok(())
    }

    /// Register a route from pre-computed segments (v0.2.1 bug fix for multi-segment routes)
    /// This bypasses path concatenation to avoid false collinearity detection in manhattan_path_to_segments
    fn register_analytic_route_from_segments(
        &mut self,
        net_id: NetId,
        net_name: &str,
        segments: Vec<hwc_engine::LineSegment>,
        routing_layer_name: &str,
        thickness_nm: i64,
        declared_width_nm: Option<i64>,
        current_limit_ma: f64,
    ) -> Result<(), IrError> {
        use hwc_engine::AnalyticTrace;

        if segments.is_empty() {
            return Ok(());
        }

        let min_width_nm = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Analytic route requires trace width constraint but none is loaded.".into(),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        let trace_width_nm = declared_width_nm.unwrap_or(min_width_nm);

        // Material determination using routing layer
        let material_id = self
            .profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup
                    .layers
                    .iter()
                    .find(|l| l.name.name == routing_layer_name)
                    .map(|l| l.material.clone())
            })
            .ok_or_else(|| IrError::UndeclaredMaterial {
                material: format!(
                    "No material defined for routing layer '{}'",
                    routing_layer_name
                )
                .into(),
            })
            .and_then(|mat_name| {
                self.space
                    .material_registry
                    .get_id(&mat_name)
                    .ok_or_else(|| IrError::UndeclaredMaterial {
                        material: mat_name.clone(),
                    })
                    .map(|id| {
                        eprintln!(
                            "[REGISTRY MATERIAL DEBUG] Net '{}': routing_layer='{}', material='{}', material_id={}",
                            net_name, routing_layer_name, mat_name, id
                        );
                        id
                    })
            })?;

        let net_actual_current_ma = self
            .space
            .netlist
            .get_net(net_id)
            .and_then(|n| n.current_ma)
            .unwrap_or(0.0);

        // Compute layer_z_range for horizontal traces
        let layer_z_range = segments
            .iter()
            .find(|s| s.start.z == s.end.z)
            .and_then(|s| self.space.find_layer_at_z(s.start.z))
            .map(|layer| (layer.z_bottom, layer.z_top));

        let trace = AnalyticTrace::with_layer_z_range(
            net_id,
            hwc_engine::space::CrossSection::new(trace_width_nm, thickness_nm),
            segments,
            material_id,
            net_name.into(),
            hwc_engine::space::CurrentRating::new(net_actual_current_ma, current_limit_ma),
            layer_z_range,
            routing_layer_name.into(),  // v0.2.2: Explicit layer lineage
        );

        let from_entity = format!("auto_route_{}_start", net_name);
        let to_entity = format!("auto_route_{}_end", net_name);

        self.space.routing_database.register_autorouter_route(
            trace,
            from_entity.into(),
            to_entity.into(),
        ).map_err(|e| IrError::RoutingError(e))?;

        Ok(())
    }
}

