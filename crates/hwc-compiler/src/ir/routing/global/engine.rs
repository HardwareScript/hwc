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

        for intent_name in data.net_intents.values() {
            if !geo_router.has_intent_composer(intent_name.as_str()) {
                // CIR Phase 2.2: Look up intent from profile-declared intents.
                // No hardcoded intents — everything comes from user-facing .hw files.
                let intent = self
                    .profile
                    .and_then(|p| p.intents.iter().find(|pi| pi.name.name == *intent_name))
                    .map(|pi| {
                        let cost_weights =
                            pi.cost_weights
                                .as_ref()
                                .map(|cw| hwc_engine::IntentCostWeights {
                                    base_cost: cw.base,
                                    via_penalty: cw.via_penalty,
                                    direction_penalty: cw.direction_penalty,
                                    tight_clearance_penalty: cw.tight_clearance_penalty,
                                    crosstalk_penalty: cw.crosstalk_penalty,
                                    impedance_penalty: cw.impedance_penalty,
                                    reference_void_penalty: cw.reference_void_penalty,
                                });
                        // NATIVE FIX: Extract escape_stub from profile intent
                        let escape_stub_nm = pi.escape_stub.as_ref().map(|m| {
                            // Simple unit conversion - profile measurements are literals
                            match m.unit {
                                hwc_parser::Unit::Nanometer => m.value as i64,
                                hwc_parser::Unit::Micrometer => (m.value * 1_000.0) as i64,
                                hwc_parser::Unit::Millimeter => (m.value * 1_000_000.0) as i64,
                                hwc_parser::Unit::Centimeter => (m.value * 10_000_000.0) as i64,
                                _ => panic!("Invalid unit for escape_stub: {:?}", m.unit),
                            }
                        });
                        hwc_engine::RoutingIntent::from_profile_data(
                            &pi.name.name,
                            pi.routing_style.as_ref().map(|s| s.name.as_str()),
                            cost_weights.as_ref(),
                            escape_stub_nm,
                        )
                    })
                    .unwrap_or_else(|| hwc_engine::RoutingIntent::new(intent_name));

                let via_penalty = self
                    .profile
                    .and_then(|p| p.routing.as_ref())
                    .and_then(|r| r.via_penalty)
                    .unwrap_or(50);
                let direction_penalty = self
                    .profile
                    .and_then(|p| p.routing.as_ref())
                    .and_then(|r| r.direction_penalty)
                    .unwrap_or(10);
                let crosstalk_penalty = self
                    .profile
                    .and_then(|p| p.routing.as_ref())
                    .and_then(|r| r.crosstalk_penalty)
                    .unwrap_or(3);
                let reference_void_penalty = self
                    .profile
                    .and_then(|p| p.routing.as_ref())
                    .and_then(|r| r.reference_void_penalty)
                    .unwrap_or(5_000_000);
                geo_router.register_intent_composer(
                    intent_name.clone(),
                    hwc_engine::CostComposer::from_intent_overrides(
                        intent
                            .cost_weights
                            .as_ref()
                            .and_then(|w| w.via_penalty)
                            .unwrap_or(via_penalty),
                        intent
                            .cost_weights
                            .as_ref()
                            .and_then(|w| w.direction_penalty)
                            .unwrap_or(direction_penalty),
                        intent
                            .cost_weights
                            .as_ref()
                            .and_then(|w| w.crosstalk_penalty)
                            .unwrap_or(crosstalk_penalty),
                        intent
                            .cost_weights
                            .as_ref()
                            .and_then(|w| w.reference_void_penalty)
                            .unwrap_or(reference_void_penalty),
                    ),
                );
            }
        }

        // v0.1.9: Extract explicit segments WITH normals for perpendicular escape
        let mut explicit_segments: Vec<(NetId, Vec<Point3D>)> = Vec::new();
        let mut net_normals: FxHashMap<NetId, (hwc_engine::geometry_router::connection_interface::Normal2D, hwc_engine::geometry_router::connection_interface::Normal2D)> = FxHashMap::default();
        let mut net_escape_stubs: FxHashMap<NetId, i64> = FxHashMap::default();

        // v0.1.9: Build a map from route index to escape_stub from original parser routes
        let route_escape_stubs: Vec<Option<i64>> = self.config.auto_routes
            .iter()
            .map(|route| {
                route.escape_stub.as_ref().and_then(|expr| {
                    self.evaluate_escape_stub_expression(expr).ok()
                })
            })
            .collect();
        
        // NATIVE FIX: Get global default escape_stub from profile (REQUIRED)
        // Profile measurements don't contain pdk.* references, so we can use simple conversion
        let global_escape_stub_nm = self
            .profile
            .and_then(|p| p.routing.as_ref())
            .and_then(|r| r.escape_stub.as_ref())
            .map(|m| {
                // Simple unit conversion - profile measurements are literals
                match m.unit {
                    hwc_parser::Unit::Nanometer => m.value as i64,
                    hwc_parser::Unit::Micrometer => (m.value * 1_000.0) as i64,
                    hwc_parser::Unit::Millimeter => (m.value * 1_000_000.0) as i64,
                    hwc_parser::Unit::Centimeter => (m.value * 10_000_000.0) as i64,
                    _ => panic!("Invalid unit for escape_stub: {:?}", m.unit),
                }
            })
            .ok_or_else(|| IrError::MissingProfileConstraint {
                field: "routing.escape_stub".into(),
            })?;

        for (idx, resolved) in data.resolved_routes.iter().enumerate() {
            match crate::ir::routing::resolve_route_boundary_points(self.space, resolved, resolved.width_nm) {
                Ok((start, goal, start_normal, goal_normal)) => {
                    eprintln!("[ENGINE DEBUG] Route {} ({}): boundary resolution returned start=({},{},{}), goal=({},{},{})",
                        idx, resolved.net_name, start.x, start.y, start.z, goal.x, goal.y, goal.z);
                    
                    explicit_segments.push((resolved.net_id, vec![start, goal]));
                    
                    eprintln!("[ENGINE DEBUG] Route {} ({}): pushed to explicit_segments",
                        idx, resolved.net_name);
                    
                    // Convert Point3D normals (i64) to Normal2D (i32) - safe for unit vectors scaled by 10^9
                    let start_normal_2d = hwc_engine::geometry_router::connection_interface::Normal2D {
                        x: start_normal.x as i32,
                        y: start_normal.y as i32,
                    };
                    let goal_normal_2d = hwc_engine::geometry_router::connection_interface::Normal2D {
                        x: goal_normal.x as i32,
                        y: goal_normal.y as i32,
                    };
                    net_normals.insert(resolved.net_id, (start_normal_2d, goal_normal_2d));
                    
                    // v0.1.9 NATIVE FIX: Resolve escape_stub with proper authority hierarchy:
                    // 1. Route-specific override (highest priority)
                    // 2. Intent-based override (from net_type)
                    // 3. Profile global default (required, no fallback)
                    let escape_stub_nm = if let Some(Some(route_override)) = route_escape_stubs.get(idx) {
                        // Route-specific escape_stub takes highest priority
                       
                        *route_override
                    } else if let Some(intent_name) = data.net_intents.get(&resolved.net_name) {
                       
                        // Look up intent's escape_stub
                        self.profile
                            .and_then(|p| p.intents.iter().find(|pi| pi.name.name == *intent_name))
                            .and_then(|pi| {
                               
                                pi.escape_stub.as_ref()
                            })
                            .map(|m| {
                                // Simple unit conversion - profile measurements are literals
                                let nm = match m.unit {
                                    hwc_parser::Unit::Nanometer => m.value as i64,
                                    hwc_parser::Unit::Micrometer => (m.value * 1_000.0) as i64,
                                    hwc_parser::Unit::Millimeter => (m.value * 1_000_000.0) as i64,
                                    hwc_parser::Unit::Centimeter => (m.value * 10_000_000.0) as i64,
                                    _ => panic!("Invalid unit for escape_stub: {:?}", m.unit),
                                };
                               
                                nm
                            })
                            .unwrap_or_else(|| {
                                
                                global_escape_stub_nm
                            })
                    } else {
                        // No route override, no intent -> use global (which is required to exist)
                       
                        global_escape_stub_nm
                    };
                    
                    net_escape_stubs.insert(resolved.net_id, escape_stub_nm);
                   
                }
                Err(e) => {
                    eprintln!("[ROUTER WARNING] Failed to resolve boundary points for net '{}': {:?} - skipping", resolved.net_name, e);
                }
            }
        }

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

        eprintln!("[ENGINE DEBUG] About to call route_space with {} explicit segments:", explicit_segments.len());
        for (i, (net_id, points)) in explicit_segments.iter().enumerate() {
            eprintln!("[ENGINE DEBUG]   Segment {} (net {:?}): {} points", i, net_id, points.len());
            for (j, p) in points.iter().enumerate() {
                eprintln!("[ENGINE DEBUG]     Point {}: ({},{},{})", j, p.x, p.y, p.z);
            }
        }

        // Snapshot substrate layers from the entity graph before passing a mutable
        // borrow of the entity graph to route_space (avoids an aliasing borrow).
        let substrate_layers_owned = self.space.entity_graph.get_substrate_layers().to_vec();
        let substrate_layers = if substrate_layers_owned.is_empty() {
            None
        } else {
            Some(substrate_layers_owned.as_slice())
        };

        geo_router
            .route_space(
                RouteSpaceRequest {
                    grid_bbox: &grid_bbox,
                    nets: &FxHashMap::default(),
                    explicit_segments: Some(&explicit_segments),
                    obstacle_bboxes: &data.obstacle_bboxes,
                    substrate_layers,
                    net_frequencies: &self.config.net_frequencies,
                    net_trace_widths: &net_trace_widths_by_id,
                    net_normals: if !net_normals.is_empty() {
                        Some(&net_normals)
                    } else {
                        None
                    },
                    net_escape_stubs: if !net_escape_stubs.is_empty() {
                        Some(&net_escape_stubs)
                    } else {
                        None
                    },
                    net_layer_targets: if !data.net_layer_targets_by_id.is_empty() {
                        Some(&data.net_layer_targets_by_id)
                    } else {
                        None
                    },
                },
                &mut self.space.entity_graph,
            )
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
                &mut self.space.entity_graph,
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

impl<'a> AutoRouter<'a> {
    /// v0.1.9: Helper to evaluate escape_stub expression to nanometers
    fn evaluate_escape_stub_expression(&self, expr: &hwc_parser::Expression) -> Result<i64, IrError> {
        match expr {
            hwc_parser::Expression::Literal { value, .. } => Ok(*value),
            hwc_parser::Expression::FloatLiteral { value, .. } => Ok(*value as i64),
            hwc_parser::Expression::Measurement { value, unit, .. } => {
                let nm = match unit {
                    hwc_parser::Unit::Nanometer => *value as i64,
                    hwc_parser::Unit::Micrometer => (*value * 1000.0) as i64,
                    hwc_parser::Unit::Millimeter => (*value * 1_000_000.0) as i64,
                    hwc_parser::Unit::Centimeter => (*value * 10_000_000.0) as i64,
                    _ => return Err(IrError::InvalidRouteExpression {
                        expression: format!("{:?}", expr),
                        reason: format!("Invalid unit {:?} for escape_stub - must be a distance unit (nm, um, mm, cm)", unit),
                    }),
                };
                Ok(nm)
            },
            _ => Err(IrError::InvalidRouteExpression {
                expression: format!("{:?}", expr),
                reason: "escape_stub must be a measurement expression (e.g., '500nm', '1um')".into(),
            }),
        }
    }

}
