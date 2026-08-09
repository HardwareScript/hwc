mod depth_resolver;
mod helpers;
mod netlist_ops;
mod place_drilled;
mod place_simple;
mod resolve;

use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::layer_connection_database::ViaRegistrationParams;
use hwc_engine::{HardwareSpace, Point3D};
use hwc_physics::geometry::Point2D;

use helpers::*;
use netlist_ops::*;
use resolve::*;

/// Parameters for `place_contact` function to avoid too many arguments.
pub struct PlaceContactParams<'a> {
    pub space: &'a mut HardwareSpace,
    pub contact: &'a hwc_parser::ContactPlacement,
    pub origin: hwc_parser::OriginPoint,
    pub symbol_table: &'a crate::SymbolTable,
    pub eval_context: &'a hwc_parser::EvaluationContext,
    pub stackup_manager: &'a StackupManager,
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
    pub bbox_tracker: &'a crate::BoundingBoxTracker,
}

pub fn place_contact(params: PlaceContactParams) -> Result<(), IrError> {
    // Destructure immediately: zero-cost, and the body below reads exactly as it
    // did when these were separate function parameters.
    let PlaceContactParams {
        space,
        contact,
        origin,
        symbol_table,
        eval_context,
        stackup_manager,
        profile,
        bbox_tracker,
    } = params;

    let material_id = space
        .material_registry
        .get_id(&contact.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: contact.material.clone(),
        })?;

    // v0.2.0: Resolve position from either absolute coordinates or relational anchor
    let xy_point = if let Some(ref anchor) = contact.relational_anchor {
        // Resolve relational anchor (e.g., Region.center) - returns 2D point
        resolve_relational_anchor(anchor, bbox_tracker, &contact.name)?
    } else if let Some(ref position) = contact.position {
        if position.is_relative() {
            let solver =
                crate::constraint_solver::ConstraintSolver::new(bbox_tracker, eval_context);
            let intent = solver.resolve_position(position).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("contact '{}' position", contact.name.base.as_str()),
                    reason: e.to_string(),
                }
            })?;
            let point_3d = intent.point();
            Point2D::new(point_3d.x, point_3d.y)
        } else {
            let ctx = crate::ir::conversions::CoordinateContext {
                origin,
                space_dimensions: &space.dimensions,
                symbol_table,
                eval_context,
                bbox_tracker: Some(bbox_tracker),
                stackup_manager,
                profile,
            };
            let point_3d =
                crate::ir::conversions::coordinate_to_point(position, &ctx).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!(
                            "contact '{}' position",
                            contact.name.base.as_str()
                        ),
                        reason: e,
                    }
                })?;
            // Extract 2D point from 3D
            Point2D::new(point_3d.x, point_3d.y)
        }
    } else if !contact.relational_constraints.is_empty() {
        // v0.2.1: Relational constraints present - will be resolved later
        // Skip placement for now, the relational resolver will handle it
        return Ok(());
    } else {
        return Err(IrError::PlacementConstraint {
            message: "Contact must specify either absolute position, relational anchor, or relational constraints (align:, right_of:, etc.)".into(),
            component: contact.name.base.to_string(),
        });
    };

    println!(
        "[PLACE_CONTACT_DEBUG] Resolved xy_point for '{}': x={}, y={}",
        contact.name.base.as_str(),
        xy_point.x,
        xy_point.y
    );

    let diameter_nm = get_prop_nm(contact, "drill_diameter", symbol_table, eval_context)
        .or_else(|| get_prop_nm(contact, "diameter", symbol_table, eval_context))
        .or_else(|| {
            profile
                .and_then(|p| p.via.as_ref())
                .and_then(|v| v.default_diameter.as_ref())
                .and_then(|d| crate::ir::conversions::measurement_to_nm(d, symbol_table, eval_context).ok())
        })
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: format!(
                "Contact '{}' has no explicit diameter and no profile default via diameter.",
                contact.name.as_str()
            ),
            hint: "Add 'diameter: <value>' to the contact, or declare 'via: default_diameter: <value>' in the profile.".into(),
        })?;
    let radius_nm = diameter_nm / 2;

    let from_bottom_nm = stackup_manager.resolve_elevation_bottom(
        &contact.from_elevation,
        symbol_table,
        eval_context,
        space.resolution_nm,
    )?;
    let to_bottom_nm = stackup_manager.resolve_elevation_bottom(
        &contact.to_elevation,
        symbol_table,
        eval_context,
        space.resolution_nm,
    )?;
    let from_top_nm = stackup_manager.resolve_elevation_top(
        &contact.from_elevation,
        symbol_table,
        eval_context,
    )?;
    let to_top_nm =
        stackup_manager.resolve_elevation_top(&contact.to_elevation, symbol_table, eval_context)?;

    let contact_name_debug = contact.name.base.as_str();
    println!("[PLACE_CONTACT] '{}' material='{}' dia={}nm from_z={}nm to_z={}nm from_top={}nm to_top={}nm",
        contact_name_debug, contact.material, diameter_nm,
        from_bottom_nm, to_bottom_nm, from_top_nm, to_top_nm);

    // v0.2.1: Resolve layer materials and thicknesses for depth calculation
    let (lower_layer_name, lower_bottom, lower_top, upper_layer_name, upper_bottom, upper_top) =
        if from_bottom_nm < to_bottom_nm {
            (
                stackup_manager.get_layer_name(&contact.from_elevation),
                from_bottom_nm,
                from_top_nm,
                stackup_manager.get_layer_name(&contact.to_elevation),
                to_bottom_nm,
                to_top_nm,
            )
        } else {
            (
                stackup_manager.get_layer_name(&contact.to_elevation),
                to_bottom_nm,
                to_top_nm,
                stackup_manager.get_layer_name(&contact.from_elevation),
                from_bottom_nm,
                from_top_nm,
            )
        };

    let lower_layer_name = lower_layer_name.ok_or_else(|| IrError::PlacementConstraint {
        message: format!(
            "Could not resolve lower layer name for contact '{}'",
            contact_name_debug
        ),
        component: contact_name_debug.to_string(),
    })?;

    let upper_layer_name = upper_layer_name.ok_or_else(|| IrError::PlacementConstraint {
        message: format!(
            "Could not resolve upper layer name for contact '{}'",
            contact_name_debug
        ),
        component: contact_name_debug.to_string(),
    })?;

    let lower_thickness_nm = lower_top - lower_bottom;
    let upper_thickness_nm = upper_top - upper_bottom;

    // Get layer materials from stackup
    let lower_material = stackup_manager
        .get_layer_material(&lower_layer_name)
        .ok_or_else(|| IrError::PlacementConstraint {
            message: format!(
                "Could not resolve material for layer '{}'",
                lower_layer_name
            ),
            component: contact_name_debug.to_string(),
        })?;

    let upper_material = stackup_manager
        .get_layer_material(&upper_layer_name)
        .ok_or_else(|| IrError::PlacementConstraint {
            message: format!(
                "Could not resolve material for layer '{}'",
                upper_layer_name
            ),
            component: contact_name_debug.to_string(),
        })?;

    println!(
        "[PLACE_CONTACT] '{}' layers: lower='{}' ({}, {}nm thick) upper='{}' ({}, {}nm thick)",
        contact_name_debug,
        lower_layer_name,
        lower_material,
        lower_thickness_nm,
        upper_layer_name,
        upper_material,
        upper_thickness_nm
    );

    // v0.2.1: Get safety bounds from profile
    let min_depth_nm = profile
        .and_then(|p| p.via.as_ref())
        .and_then(|v| v.min_contact_depth.as_ref())
        .and_then(|m| {
            crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context).ok()
        });

    let max_depth_nm = profile
        .and_then(|p| p.via.as_ref())
        .and_then(|v| v.max_contact_depth.as_ref())
        .and_then(|m| {
            crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context).ok()
        });

    // v0.2.1: Create depth evaluation context
    let depth_context = depth_resolver::DepthEvaluationContext {
        lower_layer_thickness_nm: lower_thickness_nm,
        upper_layer_thickness_nm: upper_thickness_nm,
        min_depth_nm,
        max_depth_nm,
    };

    // v0.2.1: Resolve depths using material-aware lookup
    let (lower_depth_nm, upper_depth_nm) =
        depth_resolver::resolve_contact_depths(depth_resolver::ContactDepthParams {
            contact,
            lower_layer_name: &lower_layer_name,
            lower_layer_thickness_nm: lower_thickness_nm,
            lower_material,
            upper_layer_name: &upper_layer_name,
            upper_layer_thickness_nm: upper_thickness_nm,
            upper_material,
            profile: profile.ok_or_else(|| IrError::MissingAsicConstraint {
                message: format!(
                    "Contact '{}' requires a profile definition",
                    contact_name_debug
                ),
                hint: "Add a profile to your space definition".into(),
            })?,
            context: &depth_context,
        })?;

    println!(
        "[PLACE_CONTACT] '{}' resolved depths: lower={}nm upper={}nm",
        contact_name_debug, lower_depth_nm, upper_depth_nm
    );

    // v0.2.1: VALIDATION - Prevent depth exceeding layer thickness
    if lower_depth_nm > lower_thickness_nm {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Via '{}' lower depth ({}nm) exceeds lower layer '{}' thickness ({}nm). \
                 Reduce contact_depth or use percentage (e.g., 50% or 100% for complete penetration).",
                contact_name_debug, lower_depth_nm, lower_layer_name, lower_thickness_nm
            ),
            component: contact_name_debug.to_string(),
        });
    }

    if upper_depth_nm > upper_thickness_nm {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Via '{}' upper depth ({}nm) exceeds upper layer '{}' thickness ({}nm). \
                 Reduce contact_depth or use percentage (e.g., 50% or 100% for complete penetration).",
                contact_name_debug, upper_depth_nm, upper_layer_name, upper_thickness_nm
            ),
            component: contact_name_debug.to_string(),
        });
    }

    // Calculate via Z-span using resolved depths
    let via_bottom = (lower_top - lower_depth_nm).max(0); // Clamp to substrate base
    let via_top = upper_bottom + upper_depth_nm;

    let (start_z, end_z) = (via_bottom, via_top);

    println!(
        "[PLACE_CONTACT] '{}' final span: start_z={}nm end_z={}nm ({}nm tall)",
        contact_name_debug,
        start_z,
        end_z,
        end_z - start_z
    );

    // v0.2.1: VALIDATION - Prevent substrate penetration
    // Vias must not extend below the substrate base (Z=0) or above the space depth
    if start_z < 0 {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Via '{}' extends below substrate base (Z={}nm < 0nm). \
                 Vias cannot penetrate below the wafer. \
                 Reduce lower layer penetration depth (currently {}nm into '{}') or adjust layer thicknesses.",
                contact_name_debug, start_z, lower_depth_nm, lower_layer_name
            ),
            component: contact_name_debug.to_string(),
        });
    }

    if end_z > space.dimensions.depth_nm {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Via '{}' extends above space depth (Z={}nm > {}nm). \
                 Increase space dimensions.z or reduce upper layer penetration depth (currently {}nm into '{}').",
                contact_name_debug, end_z, space.dimensions.depth_nm, upper_depth_nm, upper_layer_name
            ),
            component: contact_name_debug.to_string(),
        });
    }

    let start_point = Point3D::new(xy_point.x - radius_nm, xy_point.y - radius_nm, start_z);
    let end_point = Point3D::new(xy_point.x + radius_nm, xy_point.y + radius_nm, end_z);

    let contact_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - radius_nm,
            xy_point.y - radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + radius_nm,
            xy_point.y + radius_nm,
            start_z.max(end_z),
        ),
    );

    println!("[PLACE_CONTACT_DEBUG] '{}' contact_bbox calculated: min=({},{},{}) max=({},{},{}), radius={}nm",
        contact_name_debug,
        contact_bbox.min.x, contact_bbox.min.y, contact_bbox.min.z,
        contact_bbox.max.x, contact_bbox.max.y, contact_bbox.max.z,
        radius_nm);

    check_material_collisions(space, contact, &contact_bbox, from_bottom_nm, to_bottom_nm)?;

    let net_id = resolve_net_id(space, contact)?;

    let bridge_material_name = get_prop_string(contact, "bridge", eval_context);
    let bridge_material_id = if let Some(b) = &bridge_material_name {
        Some(
            space
                .material_registry
                .get_id(b)
                .ok_or_else(|| IrError::UndeclaredMaterial {
                    material: b.clone(),
                })?,
        )
    } else {
        None
    };

    let contour = resolve_shape(contact, eval_context, symbol_table, diameter_nm, profile)?;

    let annular_ring_nm = resolve_annular_ring(space, contact, symbol_table, eval_context)?;

    let board_max_z_nm = space.dimensions.depth_nm;
    let via_net_id = hwc_engine::netlist::NetId::new(net_id);

    let via = hwc_engine::geometry_router::Via::new(hwc_engine::geometry_router::ViaSpec {
        position: (xy_point.x, xy_point.y),
        from_z_nm: start_z,
        to_z_nm: end_z,
        diameter_nm,
        net_id: via_net_id,
        material_id,
        annular_ring_nm,
        board_min_z_nm: 0,
        board_max_z_nm,
    });
    space.add_vias(vec![via]);

    let is_tented = get_prop_bool(contact, "is_tented", eval_context).unwrap_or(false);
    let pad_diameter_nm = diameter_nm + (2 * annular_ring_nm);
    let pad_radius_nm = pad_diameter_nm / 2;
    let pad_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - pad_radius_nm,
            xy_point.y - pad_radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + pad_radius_nm,
            xy_point.y + pad_radius_nm,
            start_z.max(end_z),
        ),
    );

    let liner_material_name = get_prop_string(contact, "liner", eval_context);
    let clearance_nm = resolve_clearance(space)?;
    if liner_material_name.is_some() {
        place_simple::place_tsv(
            space,
            &place_simple::SimpleViaCtx {
                contact,
                material_id,
                net_id,
                contact_bbox,
                diameter_nm,
                start_point,
                end_point,
                start_z,
                end_z,
                contour,
                symbol_table,
                eval_context,
                contact_name_debug: contact_name_debug.into(),
                is_tented,
                clearance_nm,
                resolution_nm: space.resolution_nm,
            },
            bridge_material_id,
        )?;
    } else if let Some(bridge_mat) = bridge_material_id {
        place_simple::place_compound_via(
            space,
            &place_simple::SimpleViaCtx {
                contact,
                material_id,
                net_id,
                contact_bbox,
                diameter_nm,
                start_point,
                end_point,
                start_z,
                end_z,
                contour,
                symbol_table,
                eval_context,
                contact_name_debug: contact_name_debug.into(),
                is_tented,
                clearance_nm,
                resolution_nm: space.resolution_nm,
            },
            bridge_mat,
        )?;
    } else {
        // NO DEFAULTS: Material MUST have an explicit process declaration
        let material_def = symbol_table.get_material(&contact.material).map_err(|_| {
            IrError::UndeclaredMaterial {
                material: contact.material.clone(),
            }
        })?;

        // Process is now a required field (not Option), validated at parse time
        let process = match material_def.process {
            hwc_parser::ManufacturingProcess::DrilledPlated => {
                hwc_engine::ManufacturingProcess::DrilledPlated
            }
            hwc_parser::ManufacturingProcess::Etched => hwc_engine::ManufacturingProcess::Etched,
            hwc_parser::ManufacturingProcess::Deposited => {
                hwc_engine::ManufacturingProcess::Deposited
            }
        };

        space.material_registry.set_process(material_id, process);

        println!(
            "[PLACE_CONTACT] '{}' process={:?}, net_id={}, material_id={}",
            contact_name_debug, process, net_id, material_id
        );

        if process == hwc_engine::ManufacturingProcess::DrilledPlated {
            place_drilled::place_drilled_via(place_drilled::DrilledViaPlacement {
                space,
                contact,
                material_id,
                contact_bbox,
                diameter_nm,
                net_id,
                contact_name_debug,
                symbol_table,
                eval_context,
                pad_bbox,
                is_tented,
                pad_diameter_nm,
                clearance_nm,
            })?;
        } else if process == hwc_engine::ManufacturingProcess::Etched {
            place_simple::place_etched_via(
                space,
                contact_bbox,
                diameter_nm,
                clearance_nm,
                contact_name_debug,
            )?;
        } else {
            place_simple::place_deposited_via(
                space,
                &place_simple::SimpleViaCtx {
                    contact,
                    material_id,
                    net_id,
                    contact_bbox,
                    diameter_nm,
                    start_point,
                    end_point,
                    start_z,
                    end_z,
                    contour,
                    symbol_table,
                    eval_context,
                    contact_name_debug: contact_name_debug.into(),
                    is_tented,
                    clearance_nm,
                    resolution_nm: space.resolution_nm,
                },
            )?;
        }
    }

    register_contact_in_netlist(netlist_ops::NetlistRegistration {
        space,
        contact,
        diameter_nm,
        material_id,
        xy_point: Point3D::new(xy_point.x, xy_point.y, 0), // Convert Point2D to Point3D
        start_z,
        end_z,
        symbol_table,
        eval_context,
    })?;

    // v0.2.0: Register contact as a routable entity
    let contact_name_str = contact.name.base.as_str();
    let net_id = contact
        .net
        .as_ref()
        .and_then(|net| space.netlist.get_net_by_name(net.base.as_str()));

    // v0.2.0: Register via connections in the layer connection database
    // Use the via-layer mapping database to determine exact connection Z values
    // instead of guessing from bounding box midpoints.
    {
        let from_mat_id = space.material_registry.get_id(&contact.material);
        let to_mat_id = bridge_material_name
            .as_ref()
            .and_then(|b| space.material_registry.get_id(b));

        // Register connection points for the layers this via spans
        // The from_elevation and to_elevation give us the semantic layer names
        let from_layer_name = match &contact.from_elevation {
            hwc_parser::ast::Elevation::Semantic(ident) => Some(ident.name.as_str()),
            _ => None,
        };
        let to_layer_name = match &contact.to_elevation {
            hwc_parser::ast::Elevation::Semantic(ident) => Some(ident.name.as_str()),
            _ => None,
        };

        if let (Some(from_name), Some(to_name)) = (from_layer_name, to_layer_name) {
            // CRITICAL FIX: Via connections are at layer interfaces, not layer bottoms
            // Bottom connection = TOP of the FROM layer (where via exits the lower layer)
            // Top connection = BOTTOM of the TO layer (where via enters the upper layer)
            let bottom_connection_z = from_top_nm; // Top of "active" layer
            let top_connection_z = to_bottom_nm; // Bottom of "metal1" layer

            let bottom_mat = from_mat_id.unwrap_or(0);
            let top_mat = to_mat_id.unwrap_or(from_mat_id.unwrap_or(0));

            if let Err(e) = space
                .layer_connection_db
                .register_via(ViaRegistrationParams {
                    entity_name: contact_name_str,
                    bottom_layer: from_name,
                    bottom_z: bottom_connection_z,
                    top_layer: to_name,
                    top_z: top_connection_z,
                    position_2d: (xy_point.x, xy_point.y),
                    bottom_material: bottom_mat,
                    top_material: top_mat,
                })
            {
                eprintln!(
                    "[PLACE_CONTACT] WARNING: Failed to register via connections for '{}': {}",
                    contact_name_str, e
                );
            } else {
                eprintln!(
                    "[PLACE_CONTACT] Registered via connections for '{}': {} @ {}nm -> {} @ {}nm",
                    contact_name_str, from_name, bottom_connection_z, to_name, top_connection_z
                );
            }

            // v0.2.0: Register via instance in ViaInstanceDatabase
            // This prevents duplicate automatic via insertion by ViaResolver
            if let Some(net) = net_id {
                let xy_bbox = (
                    contact_bbox.min.x,
                    contact_bbox.min.y,
                    contact_bbox.max.x,
                    contact_bbox.max.y,
                );
                let z_range = (start_z, end_z);

                space.via_instance_db.register(
                    contact_name_str,
                    net,
                    from_name,
                    to_name,
                    xy_bbox,
                    z_range,
                );

                eprintln!(
                    "[VIA INSTANCE DB] Registered explicit via '{}' on net {:?}: {} -> {} at ({}, {})",
                    contact_name_str, net, from_name, to_name, xy_point.x, xy_point.y
                );
            }
        }
    }

    store_contact_metadata(netlist_ops::ContactMetadataStorage {
        space,
        contact,
        from_bottom_nm,
        to_bottom_nm,
        diameter_nm,
        pad_bbox,
        is_tented,
        bridge_material_name,
        contact_name_debug,
        symbol_table,
        eval_context,
    });

    space.entity_graph.register_space_entity(
        contact_name_str,
        contact_bbox,
        net_id,
        (start_z + end_z) / 2, // Use mid-point z coordinate
    );

    // v0.2.0 CIR: Register PhysicalInterface for this contact
    {
        use hwc_engine::geometry_router::connection_interface::{
            InterfaceGeometry, PhysicalInterface,
        };
        use hwc_engine::geometry_router::routing_intent::RoutingIntent;
        use hwc_engine::netlist::ComponentId;
        use smallvec::smallvec;

        let constraints = space.fabrication_constraints.as_ref().ok_or_else(|| {
            IrError::MissingAsicConstraint {
                message: "Fabrication constraints required for contact interface generation".into(),
                hint: "Add a 'trace:' block to your profile with min_width and min_spacing".into(),
            }
        })?;

        let trace_width_nm = constraints.trace.min_width_nm;
        let clearance_nm = constraints.trace.min_spacing_nm;

        // v0.2.0: Query the layer connection database for the correct connection Z.
        // This ensures we use the stackup-derived Z, not a guessed midpoint.
        let connection_z_nm = if let Some(from_name) = match &contact.from_elevation {
            hwc_parser::ast::Elevation::Semantic(ident) => Some(ident.name.as_str()),
            _ => None,
        } {
            // Try to get the routing Z from the routing layer database
            space.routing_layer_db.get_routing_z(from_name).unwrap_or({
                // Fall back to top of via bbox if routing layer DB doesn't have this layer
                contact_bbox.max.z
            })
        } else {
            contact_bbox.max.z
        };

        use hwc_parser::OriginXY;
        let is_y_upward = matches!(origin.xy, OriginXY::BL | OriginXY::BR);

        // v0.1.9.4 BUG FIX: Use pad_bbox for interface geometry instead of contact_bbox.
        // Contact_bbox is just the drill hole, but pad_bbox includes the annular ring.
        // This ensures escape points are calculated from the pad surface, not the drill edge.
        let interface_bbox = pad_bbox;

        let geometry = if is_y_upward {
            InterfaceGeometry::Polygon(vec![
                Point3D::new(interface_bbox.min.x, interface_bbox.min.y, connection_z_nm),
                Point3D::new(interface_bbox.max.x, interface_bbox.min.y, connection_z_nm),
                Point3D::new(interface_bbox.max.x, interface_bbox.max.y, connection_z_nm),
                Point3D::new(interface_bbox.min.x, interface_bbox.max.y, connection_z_nm),
            ])
        } else {
            InterfaceGeometry::Polygon(vec![
                Point3D::new(interface_bbox.min.x, interface_bbox.min.y, connection_z_nm),
                Point3D::new(interface_bbox.min.x, interface_bbox.max.y, connection_z_nm),
                Point3D::new(interface_bbox.max.x, interface_bbox.max.y, connection_z_nm),
                Point3D::new(interface_bbox.min.x, interface_bbox.min.y, connection_z_nm),
            ])
        };

        let interface_id = space.entity_graph.allocate_interface_id();
        let intent = RoutingIntent::new("Default");
        let db =
            hwc_engine::geometry_router::connection_interface::DefaultRoutingDatabase::default();
        let pseudo_component_id = ComponentId::new(0xFFFF_0000 + interface_id.raw());

        let interface = PhysicalInterface::new(
            hwc_engine::geometry_router::connection_interface::PhysicalInterfaceParams {
                id: interface_id,
                component_id: pseudo_component_id,
                geometry,
                capabilities: smallvec![],
                routing_intent: intent,
                orientation: Some(
                    hwc_engine::geometry_router::connection_interface::Orientation::Derived,
                ),
                trace_width_nm,
                escape_stub_length_nm: clearance_nm * 2,
            },
            &db,
        );

        space
            .entity_graph
            .register_space_entity_interface(contact.name.base.clone(), interface);
    }

    println!(
        "[DEBUG] Registered contact '{}' as routing endpoint with net_id={:?}",
        contact_name_str, net_id
    );

    Ok(())
}
