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

            for path in segments {
                if path.len() < 2 {
                    continue;
                }
                let miter_engine = hwc_engine::MiterEngine::new(trace_width);
                let mitered_path = miter_engine.apply_miter_pass(path);
                let (refined_path, actual_thickness) =
                    self.refine_path_z(mitered_path, trace_thickness_nm)?;

                let mut final_path = refined_path;
                self.add_vertical_transitions(&mut final_path, &net_name, data);

                let declared_width = data
                    .net_declared_widths
                    .get::<str>(net_name.as_ref())
                    .copied();
                let current_ma = self.resolve_net_current(&net_name, data)?;

                self.register_analytic_route(
                    actual_net_id,
                    &net_name,
                    final_path,
                    actual_thickness,
                    declared_width,
                    current_ma,
                )?;
            }
        }

        self.space.entity_graph.commit_route();
        // v0.1.9.1: CRITICAL FIX - Re-register ALL routes from analytic source of truth.
        // This prevents the "Double-Registration" bug where the original straight-line path
        // coexists with the detour path in the physical database.
        crate::ir::routing::automatic::re_register_resolved_routes(self.space)?;

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

        let routing_copper_id = self.resolve_sample_copper_id()?;
        for (&net_id, mutated_paths) in &result.paths {
            self.space.entity_graph.clear_routes_for_net(net_id);
            for path in mutated_paths {
                self.space.entity_graph.register_route(
                    net_id,
                    path,
                    routing_copper_id,
                    trace_width,
                );
            }
        }
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
        let min_clearance = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Legalization requires spacing constraints.".into(),
                hint: "Add 'trace:' block.".into(),
            })?;

        let legalizer = hwc_engine::geometry_router::Legalizer::new(min_clearance);
        let all_routes = self.space.entity_graph.get_all_routes();
        let mut all_segments = Vec::new();
        let mut all_net_ids = Vec::new();
        for (net_id, segments) in all_routes {
            for seg in segments {
                all_segments.push(seg.clone());
                all_net_ids.push(*net_id);
            }
        }

        if !all_segments.is_empty() {
            let (legalized_segments, legalized_net_ids) = legalizer.legalize(
                &all_segments,
                &all_net_ids,
                &self.space.material_registry,
                self.space.entity_graph.spatial(),
                10,
            );
            let net_ids_to_clear: Vec<_> = self
                .space
                .entity_graph
                .get_all_routes()
                .iter()
                .map(|(net_id, _)| *net_id)
                .collect();
            for net_id in net_ids_to_clear {
                self.space.entity_graph.clear_routes_for_net(net_id);
            }
            for (idx, seg) in legalized_segments.iter().enumerate() {
                self.space
                    .entity_graph
                    .register_trace_segments(legalized_net_ids[idx], vec![seg.clone()]);
            }
        }
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
        let all_routes = self.space.entity_graph.get_all_routes();
        let mut new_analytic_routes = Vec::new();
        for (net_id, segments) in all_routes {
            if segments.is_empty() {
                continue;
            }
            let net_name = self
                .space
                .netlist
                .get_net(*net_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("net_{}", net_id.raw()).into());
            let width_nm = segments.first().map(|s| s.width_nm).unwrap_or(250);
            let material = segments.first().map(|s| s.material_id).unwrap_or(0);
            let thickness_nm = self
                .space
                .material_registry
                .get_material(material)
                .map(|m| m.thickness_nm)
                .unwrap_or(400);
            let line_segments = segments
                .iter()
                .map(|seg| hwc_engine::LineSegment {
                    start: seg.start,
                    end: seg.end,
                })
                .collect();
            let current_ma = self
                .space
                .netlist
                .get_net(*net_id)
                .and_then(|n| n.current_ma)
                .unwrap_or(0.0);
            new_analytic_routes.push(hwc_engine::AnalyticTrace {
                net_id: *net_id,
                cross_section: hwc_engine::space::CrossSection::new(width_nm, thickness_nm),
                segments: line_segments,
                material,
                net_name,
                current: hwc_engine::space::CurrentRating::new(current_ma, 0.0),
            });
        }
        self.space.analytic_routes = new_analytic_routes;
        Ok(())
    }
}
