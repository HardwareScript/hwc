use super::builder::RoutingData;
use super::config::AutoRouter;
use crate::ir::errors::IrError;
use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::{GridBounds, RouteResult, RouteSpaceRequest};
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;

impl<'a> AutoRouter<'a> {
    pub(crate) fn setup_and_run_engine(
        &mut self,
        data: &RoutingData,
    ) -> Result<RouteResult, IrError> {
        let grid_bounds = GridBounds::new(
            self.space.dimensions.width_nm,
            self.space.dimensions.height_nm,
            self.space.dimensions.depth_nm,
        );

        let mut constraints =
            hwc_engine::constraint_manager::ConstraintRulebook::new(self.space.resolution_nm);
        self.configure_constraints(&mut constraints)?;

        let mut geo_router = hwc_engine::GeometryRouter::new(
            grid_bounds,
            constraints,
            self.space.material_registry.clone(),
        );

        self.configure_geo_router(&mut geo_router, data)?;

        let explicit_segments: Vec<(NetId, Vec<Point3D>)> = data.resolved_routes.iter()
            .filter_map(|resolved| {
                match crate::ir::routing::resolve_route_boundary_points(self.space, resolved, resolved.width_nm) {
                    Ok((start, goal)) => Some((resolved.net_id, vec![start, goal])),
                    Err(e) => {
                        eprintln!("[ROUTER WARNING] Failed to resolve boundary points for net '{}': {:?} - skipping", resolved.net_name, e);
                        None
                    }
                }
            })
            .collect();

        if explicit_segments.is_empty() {
            return Err(IrError::RoutingError(
                "No routes could be resolved from EntityGraph.".into(),
            ));
        }

        let grid_bbox = hwc_engine::geometry::BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(
                self.space.dimensions.width_nm,
                self.space.dimensions.height_nm,
                self.space.dimensions.depth_nm,
            ),
        );

        let net_trace_widths_by_id = self.build_net_trace_widths(data);

        geo_router
            .route_space(RouteSpaceRequest {
                grid_bbox: &grid_bbox,
                nets: &FxHashMap::default(),
                explicit_segments: Some(&explicit_segments),
                obstacle_bboxes: &data.obstacle_bboxes,
                substrate_layers: if !self.space.entity_graph.get_substrate_layers().is_empty() {
                    Some(self.space.entity_graph.get_substrate_layers())
                } else {
                    None
                },
                net_frequencies: &self.config.net_frequencies,
                net_trace_widths: &net_trace_widths_by_id,
            })
            .map_err(|_| IrError::NoPathFound {
                net: "batch".into(),
                from_pin: "batch".into(),
                to_pin: "batch".into(),
            })
    }

    fn configure_constraints(
        &self,
        constraints: &mut hwc_engine::constraint_manager::ConstraintRulebook,
    ) -> Result<(), IrError> {
        if let Some(ref constraint_set) = self.space.fabrication_constraints {
            use hwc_engine::constraint_manager::{FabricationConstraints, StackupInfo};
            let stackup = constraint_set.stackup.as_ref().map(|s| StackupInfo {
                dielectric_height_nm: s.dielectric_height_nm,
                copper_thickness_nm: s.copper_thickness_nm,
                relative_permittivity: s.relative_permittivity,
                default_impedance_ohm: s.default_impedance_ohm,
            });
            let fab_constraints = FabricationConstraints {
                min_trace_width_nm: constraint_set.trace.min_width_nm,
                min_trace_spacing_nm: constraint_set.trace.min_spacing_nm,
                min_via_diameter_nm: constraint_set.via.min_diameter_nm,
                default_via_diameter_nm: constraint_set.via.default_diameter_nm,
                min_annular_ring_nm: constraint_set.via.min_annular_ring_nm,
                min_spacing_nm: constraint_set.via.min_spacing_nm,
                low_voltage_clearance_nm: constraint_set.clearance.low_voltage_nm,
                medium_voltage_clearance_nm: constraint_set.clearance.medium_voltage_nm,
                high_voltage_clearance_nm: constraint_set.clearance.high_voltage_nm,
                safety_factor: constraint_set.clearance.safety_factor,
                stackup,
                solder_mask_expansion_nm: constraint_set.solder_mask_expansion_nm,
                technology: constraint_set.technology.clone(),
            };
            constraints.set_fabrication_constraints(fab_constraints);
        }
        Ok(())
    }

    fn configure_geo_router(
        &mut self,
        geo_router: &mut hwc_engine::GeometryRouter,
        data: &RoutingData,
    ) -> Result<(), IrError> {
        let trace_width = data
            .net_declared_widths
            .values()
            .max()
            .copied()
            .or_else(|| {
                self.space
                    .fabrication_constraints
                    .as_ref()
                    .map(|c| c.trace.min_width_nm)
            })
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'trace.min_width_nm' constraint".into(),
                hint: "Add a 'trace:' block to your profile with explicit min_width.".into(),
            })?;
        let routing_copper_id = self.resolve_sample_copper_id()?;
        geo_router.set_routing_context(routing_copper_id, trace_width);

        if !self.config.route_net_policies.is_empty() {
            geo_router.set_route_net_policies(self.config.route_net_policies.clone());
        }

        if let Some(qs) = self.query_store.take() {
            geo_router.query_store = Some(qs);
        }

        if let Some(profile) = self.profile {
            let is_manhattan = profile.is_asic();
            let profile_layers = self.stackup_manager.ordered_layers();
            let mut layer_z_positions = Vec::new();
            let mut layer_materials = Vec::new();

            for name in profile_layers {
                let z = self
                    .stackup_manager
                    .get_layer_start_z(name)
                    .ok_or_else(|| IrError::InvalidRouteExpression {
                        expression: format!("stackup layer '{}'", name),
                        reason: "Layer exists in profile list but not in physical stackup.".into(),
                    })?;
                layer_z_positions.push(z);

                let mat_name = profile
                    .stackup
                    .as_ref()
                    .and_then(|s| s.layers.iter().find(|l| l.name.name == *name))
                    .map(|l| l.material.clone())
                    .ok_or_else(|| IrError::UndeclaredMaterial {
                        material: format!("No material defined for layer '{}'", name).into(),
                    })?;
                let mat_id = self
                    .space
                    .material_registry
                    .get_id(&mat_name)
                    .ok_or_else(|| IrError::UndeclaredMaterial { material: mat_name })?;
                layer_materials.push(mat_id);
            }
            geo_router.set_profile_mode(
                is_manhattan,
                profile_layers.to_vec(),
                layer_z_positions,
                layer_materials,
            );

            if let Some(routing) = &profile.routing {
                let heuristics = hwc_engine::geometry_router::RoutingHeuristics {
                    base_cost: routing.base_cost.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "base_cost".into(),
                            hint: "Add 'base_cost' to profile.".into(),
                        }
                    })?,
                    via_penalty: routing.via_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "via_penalty".into(),
                            hint: "Add 'via_penalty' to profile.".into(),
                        }
                    })?,
                    direction_penalty: routing.direction_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "direction_penalty".into(),
                            hint: "Add 'direction_penalty' to profile.".into(),
                        }
                    })?,
                    tight_clearance_penalty: routing.tight_clearance_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "tight_clearance_penalty".into(),
                            hint: "Add 'tight_clearance_penalty' to profile.".into(),
                        }
                    })?,
                    crosstalk_penalty: routing.crosstalk_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "crosstalk_penalty".into(),
                            hint: "Add 'crosstalk_penalty' to profile.".into(),
                        }
                    })?,
                    impedance_penalty: routing.impedance_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "impedance_penalty".into(),
                            hint: "Add 'impedance_penalty' to profile.".into(),
                        }
                    })?,
                    reference_void_penalty: routing.reference_void_penalty.ok_or_else(|| {
                        IrError::MissingRoutingHeuristics {
                            field: "reference_void_penalty".into(),
                            hint: "Add 'reference_void_penalty' to profile.".into(),
                        }
                    })?,
                };
                geo_router.set_routing_heuristics(heuristics);
            }
        }

        for metadata in self.space.entity_graph.get_component_metadata() {
            geo_router.add_component_obstacle(
                metadata.bbox,
                metadata.material,
                metadata.name.clone(),
                metadata.component_type.clone(),
            );
        }
        for pin in self.space.entity_graph.get_component_pins() {
            geo_router.add_component_pin(
                pin.x_nm,
                pin.y_nm,
                pin.z_nm,
                pin.component_name.clone(),
                pin.pin_name.clone(),
                pin.net.clone(),
            );
        }

        Ok(())
    }

    fn build_net_trace_widths(&self, data: &RoutingData) -> FxHashMap<NetId, i64> {
        let mut net_trace_widths_by_id = FxHashMap::default();
        for (net_name, &width_nm) in &data.net_declared_widths {
            if let Some(&net_id) =
                data.net_id_to_name
                    .iter()
                    .find_map(|(id, name)| if name == net_name { Some(id) } else { None })
            {
                net_trace_widths_by_id.insert(net_id, width_nm);
            }
        }
        net_trace_widths_by_id
    }
}
