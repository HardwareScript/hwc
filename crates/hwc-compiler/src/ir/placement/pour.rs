use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::space::PourMetadata;
use hwc_engine::{HardwareSpace, Point3D};

pub fn place_pour(
    space: &mut HardwareSpace,
    pour: &hwc_parser::PourPlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&pour.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: pour.material.clone(),
        })?;

    // v0.2.1: Boundary is optional if relational constraints OR (position + dimensions) are provided
    // The relational resolver/compiler will compute the boundary from constraints or position + dimensions
    let has_dimensions = pour.width.is_some() && pour.height.is_some();
    if pour.boundary.is_none() && pour.relational_constraints.is_empty() && !(pour.position.is_some() && has_dimensions) {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Pour '{}' missing boundary (provide either 'boundary:', 'at:' + 'dimensions:', or relational constraints like 'align:', 'right_of:', etc.)",
                pour.name
            ),
            component: pour.name.to_string().into(),
        });
    }

    // If there are relational constraints or (position + dimensions) but no boundary yet, resolve them first
    if pour.boundary.is_none() {
        // Will be resolved in this function or relational_resolver pass
        if pour.relational_constraints.is_empty() {
            // Must be position + dimensions case - resolve it now
            if let (Some(pos), Some(w), Some(h)) = (&pour.position, &pour.width, &pour.height) {
                let center = if pos.is_relative() {
                    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);
                    let intent = solver.resolve_position(pos).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("pour '{}' position", pour.name),
                            reason: e.to_string(),
                        }
                    })?;
                    intent.point()
                } else {
                    let coord_ctx = CoordinateContext {
                        origin: ctx.origin,
                        space_dimensions: &space.dimensions,
                        symbol_table: ctx.symbol_table,
                        eval_context: ctx.eval_context,
                        bbox_tracker: Some(bbox_tracker),
                        stackup_manager: ctx.stackup_manager,
                        profile: ctx.profile,
                    };
                    crate::ir::conversions::coordinate_to_point(pos, &coord_ctx).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("pour '{}' position", pour.name),
                            reason: e,
                        }
                    })?
                };
                
                // Evaluate dimensions
                let width_nm = crate::ir::conversions::evaluate_expression_to_nm(w, ctx.symbol_table, ctx.eval_context)
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' width", pour.name),
                        reason: e,
                    })?;
                let height_nm = crate::ir::conversions::evaluate_expression_to_nm(h, ctx.symbol_table, ctx.eval_context)
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' height", pour.name),
                        reason: e,
                    })?;
                
                // Create boundary from center + dimensions
                let from = Point3D::new(
                    center.x - width_nm / 2,
                    center.y - height_nm / 2,
                    center.z,
                );
                let to = Point3D::new(
                    center.x + width_nm / 2,
                    center.y + height_nm / 2,
                    center.z,
                );
                
                let span_empty = hwc_parser::Span::new(0, 0);
                
                // Create a mutable pour to place
                let mut resolved_pour = pour.clone();
                resolved_pour.boundary = Some(hwc_parser::PourBoundary::Rect(
                    Box::new(hwc_parser::Coordinate::Positional {
                        x: hwc_parser::Expression::Measurement {
                            value: from.x as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        y: hwc_parser::Expression::Measurement {
                            value: from.y as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        z: hwc_parser::Expression::Measurement {
                            value: from.z as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        span: span_empty,
                    }),
                    Box::new(hwc_parser::Coordinate::Positional {
                        x: hwc_parser::Expression::Measurement {
                            value: to.x as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        y: hwc_parser::Expression::Measurement {
                            value: to.y as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        z: hwc_parser::Expression::Measurement {
                            value: to.z as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        span: span_empty,
                    }),
                ));
                
                // Continue with placement using resolved boundary
                return place_pour(space, &resolved_pour, bbox_tracker, ctx);
            }
        } else {
            // Relational constraints present - will be resolved in relational_resolver pass
            return Ok(());
        }
    }

    let boundary = pour.boundary.as_ref().unwrap();

    let layer_name = match &pour.elevation {
        hwc_parser::Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    };

    let thickness_nm = if let Some(t_expr) = &pour.thickness {
        crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table, ctx.eval_context).map_err(
            |e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("pour '{}' thickness", pour.name),
                reason: e.to_string(),
            },
        )?
    } else {
        ctx.profile
            .and_then(|p| p.get_layer_thickness(&layer_name))
            .and_then(|t_expr| {
                crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table, ctx.eval_context).ok()
            })
            .unwrap_or_else(|| {
                ctx.stackup_manager
                    .get_layer_thickness(&layer_name)
                    .unwrap_or(0)
            })
    };

    if thickness_nm == 0 && pour.thickness.is_none() {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Could not resolve physical thickness for pour '{}' on layer '{}'. \
                 Ensure the layer is defined in the profile stackup or provide an explicit 'thickness:' property.",
                pour.name, layer_name
            ),
            component: pour.name.to_string().into(),
        });
    }

    let z_start_nm = ctx
        .stackup_manager
        .resolve_elevation(&pour.elevation, ctx.symbol_table, ctx.eval_context)?;
    let z_end_nm = z_start_nm + thickness_nm;

    /*
    eprintln!(
        "[DEBUG pour] '{}' elevation={:?} -> z_start={}nm, thickness={}nm, z_end={}nm",
        pour.name, pour.elevation, z_start_nm, thickness_nm, z_end_nm
    );
    */

    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let mut circle_radius_nm: Option<i64> = None;
    let (start, end, area_nm2) = match boundary {
        hwc_parser::PourBoundary::Rect(from_raw, to_raw) => {
            let from = if from_raw.is_relative() {
                let intent = solver.resolve_position(from_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' from position", pour.name),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(from_raw, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' from", pour.name),
                        reason: e,
                    }
                })?
            };

            let to = if to_raw.is_relative() {
                let intent = solver.resolve_position(to_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' to position", pour.name),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(to_raw, &coord_ctx, true).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' to", pour.name),
                        reason: e,
                    }
                })?
            };

            let w = (to.x - from.x).abs();
            let h = (to.y - from.y).abs();
            (from, to, w * h)
        }
        hwc_parser::PourBoundary::Circle {
            center: center_raw,
            radius,
        } => {
            let radius_nm =
                crate::ir::conversions::evaluate_expression_to_nm(radius, ctx.symbol_table, ctx.eval_context)
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' circle radius", pour.name),
                        reason: e.to_string(),
                    })?;
            circle_radius_nm = Some(radius_nm);

            let center_pt = if center_raw.is_relative() {
                let intent = solver.resolve_position(center_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' circle center", pour.name),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(center_raw, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' circle center", pour.name),
                        reason: e,
                    }
                })?
            };

            let radius_nm_f = radius_nm as f64;
            let s = Point3D::new(
                center_pt.x - radius_nm_f as i64,
                center_pt.y - radius_nm_f as i64,
                0,
            );
            let e = Point3D::new(
                center_pt.x + radius_nm_f as i64,
                center_pt.y + radius_nm_f as i64,
                0,
            );

            let w = (e.x - s.x).abs();
            let h = (e.y - s.y).abs();
            (s, e, w * h)
        }
    };

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(pour.name.to_string(), bbox, start_with_z);

    // v0.1.8: Register pour in EntityGraph for O(1) resolution
    let net_id = if let Some(net_name) = &pour.net {
        let _min_width_nm = space.fabrication_constraints.as_ref().map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'trace.min_width_nm' constraint".into(),
                hint: "Add a 'trace:' block to your profile with explicit min_width.\n\nExample:\n  trace:\n    min_width: 180nm".into(),
            })?;
        Some(space.netlist.get_or_create_net(&net_name.base))
    } else {
        None
    };

    space
        .entity_graph
        .register_space_entity(&pour.name.base, bbox, net_id, z_start_nm);

    // Register PhysicalInterface for routing connectivity
    // This enables the router to connect to pours as endpoints
    {
        use hwc_engine::geometry_router::connection_interface::{InterfaceGeometry, PhysicalInterface};
        use hwc_engine::geometry_router::routing_intent::RoutingIntent;
        use hwc_engine::netlist::ComponentId;
        use smallvec::smallvec;

        // Require fabrication constraints - no fallbacks
        let constraints = space.fabrication_constraints.as_ref()
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Fabrication constraints required for interface generation".into(),
                hint: "Add a 'trace:' block to your profile with min_width and min_spacing".into(),
            })?;
        
        let trace_width_nm = constraints.trace.min_width_nm;
        let clearance_nm = constraints.trace.min_spacing_nm;

        // Calculate middle Z for alignment with routing queries
        let middle_z_nm = (bbox.min.z + bbox.max.z) / 2;

        // Determine vertex winding order based on coordinate system origin
        use hwc_parser::OriginXY;
        let is_y_upward = matches!(ctx.origin.xy, OriginXY::BL | OriginXY::BR);
        
        let geometry = if is_y_upward {
            // CCW winding for Y-up coordinate systems (BL, BR)
            InterfaceGeometry::Polygon(vec![
                Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm),  // bottom-left
                Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm),  // bottom-right
                Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm),  // top-right
                Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm),  // top-left
            ])
        } else {
            // CW winding for Y-down coordinate systems (TL, TR)
            InterfaceGeometry::Polygon(vec![
                Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm),  // top-left
                Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm),  // bottom-left
                Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm),  // bottom-right
                Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm),  // top-right
            ])
        };
        
        let interface_id = space.entity_graph.allocate_interface_id();
        
        // Routing intent must come from profile net_type declarations
        // No hardcoded defaults - explicit declarations enforce design intent
        let profile_def = ctx.profile.ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Cannot register routing interface without a profile".into(),
            hint: "Ensure the space has a profile declaration".into(),
        })?;
        
        // Build intent lookup table from profile
        let profile_intents: Vec<RoutingIntent> = profile_def
            .intents
            .iter()
            .map(|pi| {
                RoutingIntent::from_profile_data(
                    pi.name.as_str(),
                    pi.routing_style.as_ref().map(|id| id.as_str()),
                    pi.cost_weights.as_ref().map(|cw| hwc_materials::IntentCostWeights {
                        base_cost: cw.base,
                        via_penalty: cw.via_penalty,
                        direction_penalty: cw.direction_penalty,
                        tight_clearance_penalty: cw.tight_clearance_penalty,
                        crosstalk_penalty: cw.crosstalk_penalty,
                        impedance_penalty: cw.impedance_penalty,
                        reference_void_penalty: cw.reference_void_penalty,
                    }).as_ref(),
                    pi.escape_stub.as_ref().and_then(|meas| {
                        meas.to_picometers_i64().map(|pm| pm / 1000)
                    }),
                )
            })
            .collect();
        
        // Require explicit "Signal" intent declaration - no fallbacks
        let intent = RoutingIntent::lookup("Signal", &profile_intents)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Profile missing required 'Signal' net_type declaration".into(),
                hint: "Add routing intent to your profile:\n\n\
                       net_type Signal:\n    routing_style: auto\n    escape_stub: 0nm".into(),
            })?;
        
        let db = hwc_engine::geometry_router::connection_interface::DefaultRoutingDatabase::default();
        let pseudo_component_id = ComponentId::new(0xFFFF_0000 + interface_id.raw());
        
        // Pours use Derived orientation - polygon winding encodes the correct outward direction
        let interface = PhysicalInterface::new(
            interface_id,
            pseudo_component_id,
            geometry,
            smallvec![],
            intent,
            hwc_engine::geometry_router::connection_interface::Orientation::Derived,
            &db,
            trace_width_nm,
            clearance_nm * 2,
        );
        
        space.entity_graph.register_space_entity_interface(
            pour.name.base.clone(),
            interface,
        );
    }

    // v0.2.0: Register pour surface in layer connection database
    // Pours exist on a single Z plane, so they register as PourSurface type
    {
        let pour_center_x = (bbox.min.x + bbox.max.x) / 2;
        let pour_center_y = (bbox.min.y + bbox.max.y) / 2;
        // v0.2.0: Register pour at routing layer bottom Z, not at surface middle Z
        // The routing layer database expects connection points at the layer's routing elevation
        let routing_z = bbox.min.z;  // Bottom of the pour layer = routing elevation

        if let Err(e) = space.layer_connection_db.register_surface(
            &pour.name.base,
            &layer_name,
            routing_z,
            (pour_center_x, pour_center_y),
            material_id,
            hwc_engine::layer_connection_database::ConnectionType::PourSurface,
        ) {
            eprintln!(
                "[PLACE_POUR] WARNING: Failed to register pour '{}' connection: {}",
                pour.name.base, e
            );
        } else {
            eprintln!(
                "[PLACE_POUR] Registered pour '{}' surface on layer '{}' at routing Z={}nm (layer bottom)",
                pour.name.base, layer_name, routing_z
            );
        }
    }

    // println!(
    //     "   ├─ Registered pour '{}' bbox: min=({:.3}, {:.3}, {:.3}) max=({:.3}, {:.3}, {:.3})",
    //     pour.name,
    //     start_with_z.x as f64 / 1_000_000.0,
    //     start_with_z.y as f64 / 1_000_000.0,
    //     start_with_z.z as f64 / 1_000_000.0,
    //     end_with_z.x as f64 / 1_000_000.0,
    //     end_with_z.y as f64 / 1_000_000.0,
    //     end_with_z.z as f64 / 1_000_000.0,
    // );

    let skip_substrate_check = pour.waivers.merge == hwc_parser::MergeWaiver::All;

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox)
            && !skip_substrate_check
            && space.substrate_material_id != material_id
        {
            let is_conductor = space.material_registry.is_conductor(material_id);
            let is_substrate_insulator = space
                .material_registry
                .is_insulator(space.substrate_material_id)
                || space
                    .material_registry
                    .is_semiconductor(space.substrate_material_id);

            if is_conductor && is_substrate_insulator {
                let pour_net_id = if let Some(net_name) = &pour.net {
                    space
                        .netlist
                        .get_net_by_name(net_name.base.as_str())
                        .unwrap_or(hwc_engine::netlist::NetId::new(0))
                } else {
                    hwc_engine::netlist::NetId::new(0)
                };
                space.entity_graph.drill_hole(bbox, None, pour_net_id);
                println!(
                    "   ├─ Auto-carved substrate for pour '{}' ({})",
                    pour.name, pour.material
                );
            } else {
                let substrate_material_name = space
                    .material_registry
                    .get_name(space.substrate_material_id)
                    .unwrap_or("Unknown");

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Substrate interpenetration detected: Pour '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the pour outside the substrate bounds.",
                        pour.name,
                        pour.material,
                        substrate_material_name
                    ),
                    component: pour.name.to_string().into(),
                });
            }
        }
    }

    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap =
                    bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
                if z_overlap {
                    let is_waived = pour.waivers.merge == hwc_parser::MergeWaiver::All;

                    if existing.material_name != pour.material {
                        if is_waived {
                            ctx.collector
                                .report(hwc_diagnostics::WaiverApplied::new(&format!(
                                    "Pour '{}' (mat: {}) allowed to overlap '{}' (mat: {})",
                                    pour.name, pour.material, existing.name, existing.material_name
                                )));
                        } else {
                            return Err(IrError::MaterialInterpenetration {
                                pour_a: existing.name.clone(),
                                mat_a: existing.material_name.clone(),
                                pour_b: pour.name.to_string(),
                                mat_b: pour.material.clone(),
                                z_nm: z_start_nm,
                            });
                        }
                    }
                }
            }
        }
    }

    let device_binding = pour
        .device
        .as_ref()
        .map(|binding| hwc_engine::space::DeviceBinding {
            device_name: binding.device_name.clone(),
            terminal: binding.terminal.clone(),
        });

    let mut resolved_net_name = pour.net.as_ref().map(|n| n.base.clone());

    if let Some(binding) = &pour.device {
        let resolved_opt = (|| {
            let netlist = &space.netlist;
            let comp_id = netlist.get_component_by_name(binding.device_name.as_str())?;
            let pins = netlist.get_component_pins(comp_id);

            pins.iter().find_map(|&pin_id| {
                let pin_data = netlist.get_pin(pin_id)?;
                if pin_data.name == binding.terminal {
                    let net_id = pin_data.connected_net?;
                    let net_data = netlist.get_net(net_id)?;
                    Some(net_data.name.to_string())
                } else {
                    None
                }
            })
        })();

        if let Some(net_name) = resolved_opt {
            resolved_net_name = Some(net_name.into());
        }
    }

    space.pours.push(PourMetadata {
        name: pour.name.to_string(),
        material_name: pour.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2,
        bbox: Some(bbox),
        device_binding,
        merged_region_id: None,
        waivers: pour.waivers.clone(),
    });

    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        let pour_component_id = space.netlist.add_component(
            pour.name.to_string(),
            format!("Pour({})", pour.material).into(),
            (center_x, center_y, center_z),
        );

        let anchor_pin_id =
            space
                .netlist
                .add_pin(pour_component_id, "anchor".into(), (0, 0, 0), None);

        // v0.1.8: Also create a virtual pin for routing endpoint resolution.
        //
        // FIX: local_offset_nm must be (0, 0, 0) — NOT (center_x, center_y, center_z).
        // The component is already placed at the center position, so the pin
        // offset is relative to that anchor.  Using the absolute center as the
        // offset doubles the Z coordinate: get_pin_position() = comp_pos + offset
        // = center + center = 2 * center, producing e.g. Z=2900nm for a pour
        // at Z=1450nm — which falls outside the stackup and causes the
        // "No material found at Z=Xnm" error during material lookup.
        let virtual_pin_name = format!("__virtual_{}", pour.name);
        let _virtual_pin_id = space.netlist.add_pin(
            pour_component_id,
            virtual_pin_name.into(),
            (0, 0, 0),
            None,
        );

        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.clone(), 100_000, material_id)
            };

        space.netlist.connect_pin(anchor_pin_id, net_id_handle);
        space.netlist.connect_pin(_virtual_pin_id, net_id_handle);

        if let Some(binding) = &pour.device {
            if let Some(target_comp_id) = space.netlist.get_component_by_name(&binding.device_name)
            {
                if let Some(target_pin_id) = space
                    .netlist
                    .get_pin_by_name(target_comp_id, &binding.terminal)
                {
                    space.netlist.connect_pin(target_pin_id, net_id_handle);
                    // println!(
                    //     "   ├─ Bound logical pin '{}.{}' to net '{}'",
                    //     binding.device_name, binding.terminal, net_name
                    // );

                    space.entity_graph.set_pin_net(
                        &binding.device_name,
                        &binding.terminal,
                        net_name.as_str(),
                    );
                }
            }
        }

        let comp_name_for_pin = if let Some(binding) = &pour.device {
            binding.device_name.clone()
        } else {
            pour.name.to_string()
        };

        space.entity_graph.add_component_pin(
            center_x,
            center_y,
            center_z,
            comp_name_for_pin,
            "anchor".into(),
            Some(net_name.clone()),
        );

        // println!(
        //     "   ├─ Registered anchor point for pour '{}' at ({:.3}mm, {:.3}mm, {:.3}mm) on net '{}'",
        //     pour.name,
        //     center_x as f64 / 1_000_000.0,
        //     center_y as f64 / 1_000_000.0,
        //     center_z as f64 / 1_000_000.0,
        //     net_name
        // );

        net_id_handle.raw()
    } else {
        0
    };

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    // Get min_spacing from profile for early clearance validation (v0.1.9)
    // NO DEFAULTS - require explicit profile declaration
    let min_clearance_nm = space
        .fabrication_constraints
        .as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Cannot validate pour clearance without fabrication constraints".into(),
            hint: "Add a profile with 'trace: min_spacing: <value>' to enable early DRC validation"
                .into(),
        })?
        .trace
        .min_spacing_nm;

    if let Some(radius) = circle_radius_nm {
        space
            .entity_graph
            .add_circle_substrate_layer(material_id, hwc_engine::NetId::new(net_id), bbox, radius);
    } else {
        // Use checked version to catch clearance violations early (v0.1.9)
        // v0.2.1: Pass device binding for same-device terminal exemption (capacitors, etc.)
        let device_binding_ref = pour.device.as_ref().map(|b| (&b.device_name, &b.terminal));
        
        if let Err(msg) = space.entity_graph.add_substrate_layer_checked(
            material_id,
            hwc_engine::NetId::new(net_id),
            bbox,
            hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour,
            min_clearance_nm,
            device_binding_ref,
            &space.pours,
        ) {
            return Err(IrError::ClearanceViolation {
                entity_type: "pour".into(),
                entity_name: pour.name.to_string(),
                reason: format!(
                    "{}\nRequired spacing: {}nm (from profile trace.min_spacing)\n\
                     Adjust the pour boundary to maintain clearance from other nets.",
                    msg, min_clearance_nm
                )
                .into(),
            });
        }
    }

    Ok(())
}
