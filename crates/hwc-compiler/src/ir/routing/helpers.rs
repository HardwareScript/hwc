//! Shared routing utilities.

use super::super::errors::IrError;
use compact_str::CompactString;
use hwc_engine::{HardwareSpace, Point3D};

/// Generate a list of available routing endpoints for error messages
fn list_available_endpoints(space: &HardwareSpace) -> String {
    let mut endpoints = Vec::new();
    
    // Get all entities from the entity registry
    let entity_count = space.entity_graph.iter_entity_ids().count();
    eprintln!("[DEBUG list_available_endpoints] Entity count at error construction: {}", entity_count);
    
    for entity_id in space.entity_graph.iter_entity_ids() {
        // Get the entity data to access the name
        if let Ok(entity_data) = space.entity_graph.get_entity_data(*entity_id) {
            let name = entity_data.name.as_str();
            eprintln!("[DEBUG] Entity name: {} (type: {:?})", name, entity_data.entity_type);
            
            match entity_data.entity_type {
                hwc_engine::geometry_router::entity_graph::EntityType::ComponentPin => {
                    // Component pin names are formatted as "ComponentName:pin_name"
                    // We want to display them as "ComponentName.pin_name" for routing
                    if let Some((comp_name, pin_name)) = name.split_once(':') {
                        endpoints.push(format!("{}.{}", comp_name, pin_name));
                    } else {
                        endpoints.push(name.to_string());
                    }
                }
                hwc_engine::geometry_router::entity_graph::EntityType::SpacePour => {
                    // Space pours can be routed to directly by name
                    endpoints.push(name.to_string());
                }
                _ => {
                    // Other entity types (SubstrateRegion, MechanicalKeepout) are not routing endpoints
                }
            }
        }
    }
    
    eprintln!("[DEBUG] Parsed {} endpoints from {} entities", endpoints.len(), entity_count);
    
    // Always show the list, even if empty
    if endpoints.is_empty() {
        "\n\nAvailable endpoints: (none registered yet)".to_string()
    } else {
        endpoints.sort();
        endpoints.dedup();
        format!("\n\nAvailable endpoints:\n  {}", endpoints.join("\n  "))
    }
}

/// Human-readable label for a route endpoint (e.g. "M1.gate" or "VIN_Pad").
pub fn endpoint_label(endpoint: &hwc_parser::RouteEndpointSpec) -> String {
    construct_entity_name(endpoint)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Construct full entity name from route endpoint (v0.1.8)
pub fn construct_entity_name(
    endpoint: &hwc_parser::RouteEndpointSpec,
) -> Result<CompactString, IrError> {
    match endpoint {
        hwc_parser::RouteEndpointSpec::ComponentPin {
            component_name,
            component_index,
            ..
        } => {
            if let Some(ref index_expr) = component_index {
                let index_value = evaluate_index_expression(index_expr)?;
                Ok(format!("{}[{}]", component_name, index_value).into())
            } else {
                Ok(component_name.clone())
            }
        }
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, index, .. } => {
            if let Some(ref index_expr) = index {
                let index_value = evaluate_index_expression(index_expr)?;
                Ok(format!("{}[{}]", name, index_value).into())
            } else {
                Ok(name.clone())
            }
        }
    }
}

/// Evaluate an index expression to a concrete integer value
///
/// Handles:
/// - Literals: `0`, `1`, `5`
/// - Binary operations: `0 + 1` → `1`, `2 * 3` → `6`
pub fn evaluate_index_expression(expr: &hwc_parser::Expression) -> Result<i64, IrError> {
    match expr {
        hwc_parser::Expression::Literal { value, .. } => Ok(*value),
        hwc_parser::Expression::FloatLiteral { value, .. } => Ok(*value as i64),
        hwc_parser::Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let left_val = evaluate_index_expression(left)?;
            let right_val = evaluate_index_expression(right)?;

            use hwc_parser::BinaryOperator;
            match operator {
                BinaryOperator::Add => Ok(left_val + right_val),
                BinaryOperator::Subtract => Ok(left_val - right_val),
                BinaryOperator::Multiply => Ok(left_val * right_val),
                BinaryOperator::Divide => {
                    if right_val == 0 {
                        Err(IrError::InvalidExpression("Division by zero".to_string()))
                    } else {
                        Ok(left_val / right_val)
                    }
                }
                BinaryOperator::Modulo => {
                    if right_val == 0 {
                        Err(IrError::InvalidExpression("Modulo by zero".to_string()))
                    } else {
                        Ok(left_val % right_val)
                    }
                }
            }
        }
        hwc_parser::Expression::Unary {
            operator, operand, ..
        } => {
            let operand_val = evaluate_index_expression(operand)?;

            use hwc_parser::UnaryOperator;
            match operator {
                UnaryOperator::Negate => Ok(-operand_val),
                UnaryOperator::Plus => Ok(operand_val),
            }
        }
        hwc_parser::Expression::Grouped { expression, .. } => evaluate_index_expression(expression),
        _ => Err(IrError::InvalidExpression(format!(
            "Cannot evaluate non-arithmetic expression as index: {:?}",
            expr
        ))),
    }
}

/// Check if a route needs automatic routing (v0.1.7).
///
/// Returns true if the route does NOT have a manual `path:` block.
pub fn needs_automatic_routing(route: &hwc_parser::Route) -> bool {
    route.path.is_none()
}

/// Get start and goal positions from route endpoints (v0.1.8)
pub fn get_pin_positions(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(Point3D, Point3D), IrError> {
    let (start_id, goal_id) = get_pin_ids(space, route)?;

    let start_pos_tuple =
        space
            .netlist
            .get_pin_position(start_id)
            .ok_or_else(|| {
                let name = construct_entity_name(&route.from).unwrap_or_else(|_| "unknown".into());
                let available = list_available_endpoints(space);
                IrError::UnresolvedEndpoint {
                    endpoint: name.to_string(),
                    span: miette::SourceSpan::from((route.from.span().start, route.from.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                }
            })?;
    let start_pos = Point3D::new(start_pos_tuple.0, start_pos_tuple.1, start_pos_tuple.2);

    let goal_pos_tuple =
        space
            .netlist
            .get_pin_position(goal_id)
            .ok_or_else(|| {
                let name = construct_entity_name(&route.to).unwrap_or_else(|_| "unknown".into());
                let available = list_available_endpoints(space);
                IrError::UnresolvedEndpoint {
                    endpoint: name.to_string(),
                    span: miette::SourceSpan::from((route.to.span().start, route.to.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                }
            })?;
    let goal_pos = Point3D::new(goal_pos_tuple.0, goal_pos_tuple.1, goal_pos_tuple.2);

    Ok((start_pos, goal_pos))
}

/// Get start and goal pin IDs from route endpoints (v0.1.8)
pub fn get_pin_ids(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(hwc_engine::netlist::PinId, hwc_engine::netlist::PinId), IrError> {
    eprintln!("[DEBUG get_pin_ids] Called with {} entities registered", space.entity_graph.iter_entity_ids().count());
    
    let resolve_endpoint = |endpoint: &hwc_parser::RouteEndpointSpec| -> Result<hwc_engine::netlist::PinId, IrError> {
        let entity_name = construct_entity_name(endpoint)?;
        
        // v0.1.8: Use EntityGraph for O(1) resolution
        let entity_id = match endpoint {
            hwc_parser::RouteEndpointSpec::ComponentPin { .. } => {
                let full_comp_name = construct_entity_name(endpoint)?; // Reuse for index evaluation
                let pin_name = match endpoint {
                    hwc_parser::RouteEndpointSpec::ComponentPin { pin_name, pin_index, .. } => {
                        if let Some(ref idx) = pin_index {
                            let val = evaluate_index_expression(idx)?;
                            format!("{}[{}]", pin_name, val)
                        } else {
                            pin_name.to_string()
                        }
                    }
                    _ => unreachable!(),
                };
                hwc_engine::geometry::EntityId::from_str(&format!("pin:{}:{}", full_comp_name, pin_name))
            }
            hwc_parser::RouteEndpointSpec::SpaceEntity { .. } => {
                let entity_name = construct_entity_name(endpoint)?;
                eprintln!("[DEBUG] Constructing EntityId for space entity: space:{}", entity_name);
                hwc_engine::geometry::EntityId::from_str(&format!("space:{}", entity_name))
            }
        };
        
        eprintln!("[DEBUG] Looking up EntityId: {}", entity_id);

        let entity_data = space.entity_graph.get_entity_data(entity_id)
            .map_err(|_| {
                let available = list_available_endpoints(space);
                eprintln!("[DEBUG] get_entity_data FAILED for EntityId: {}", entity_id);
                IrError::UnresolvedEndpoint {
                    endpoint: entity_name.to_string(),
                    span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                }
            })?;

        eprintln!("[DEBUG] Found entity '{}', net_id: {:?}", entity_data.name, entity_data.net_id);

        // v0.1.8: Route endpoints must have a pre-assigned net (fail-fast)
        let _net_id = entity_data.net_id.ok_or_else(|| {
            eprintln!("[DEBUG] Entity '{}' has NO net_id!", entity_name);
            IrError::UnresolvedEndpoint {
                endpoint: format!("Entity '{}' has no net assignment (check PDK/Script)", entity_name),
                span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                help_message: "Ensure the entity has a net: binding in the space definition or component layout.".to_string(),
            }
        })?;
        
        eprintln!("[DEBUG] Entity '{}' has valid net_id, continuing...", entity_name);

        // In hwc-engine v0.1.8, we still need a PinId for the netlist.
        // We can find/create it based on the entity data.
        if let hwc_engine::geometry_router::entity_graph::EntityType::ComponentPin = entity_data.entity_type {
            let comp_name = match endpoint {
                hwc_parser::RouteEndpointSpec::ComponentPin { component_name, .. } => component_name.as_str(),
                _ => unreachable!(),
            };
            let comp_id = space.netlist.get_component_by_name(comp_name)
                .ok_or_else(|| IrError::PinNotFound {
                    component: comp_name.into(),
                    pin: entity_data.name.to_string(),
                })?;
            
            let pins = space.netlist.get_component_pins(comp_id);
            pins.iter()
                .find(|&&pid| {
                    if let Some(pin) = space.netlist.get_pin(pid) {
                        pin.name == entity_data.name.split('.').last().unwrap_or("")
                    } else {
                        false
                    }
                })
                .copied()
                .ok_or_else(|| IrError::PinNotFound {
                    component: comp_name.into(),
                    pin: entity_data.name.to_string(),
                })
        } else {
            // For space entities, we might need a virtual pin in the netlist
            // or handle them as direct net connections.
            // If not, we create a virtual pin for the space entity.
            let virtual_pin_name = format!("__virtual_{}", entity_name);
            let mut found_pin = None;
            for cid in 0..space.netlist.component_count() {
                if let Some(pin_id) = space.netlist.get_pin_by_name(
                    hwc_engine::netlist::ComponentId::new(cid as u32),
                    &virtual_pin_name,
                ) {
                    found_pin = Some(pin_id);
                    break;
                }
            }
            if let Some(pin_id) = found_pin {
                Ok(pin_id)
            } else {
                // This shouldn't happen if we register everything correctly during IR unrolling
                let available = list_available_endpoints(space);
                Err(IrError::UnresolvedEndpoint {
                    endpoint: entity_name.to_string(),
                    span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                })
            }
        }
    };

    let start_pin_id = resolve_endpoint(&route.from)?;
    let goal_pin_id = resolve_endpoint(&route.to)?;

    Ok((start_pin_id, goal_pin_id))
}

/// Register a net for a route and connect the source and target pins.
///
/// This ensures the netlist reflects the connectivity intent, allowing
/// the global router to discover and realize the route.
pub fn register_net_for_route(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<hwc_engine::netlist::NetId, IrError> {
    let (start_pin_id, goal_pin_id) = get_pin_ids(space, route)?;

    // Construct a unique net name if none provided
    let from_name = construct_entity_name(&route.from).unwrap_or_else(|_| "src".into());
    let to_name = construct_entity_name(&route.to).unwrap_or_else(|_| "dst".into());
    let net_name: CompactString = format!("NET_{}_to_{}", from_name, to_name).into();

    // Get trace width and material
    let width_nm = if let Some(w_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(w_expr, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "trace width".into(),
                reason: e.to_string(),
            })?
    } else {
        // v0.1.8: No hardcoded defaults. Must come from profile.
        profile.and_then(|p| p.trace.as_ref())
            .map(|t| crate::ir::conversions::measurement_to_nm(&t.min_width, symbol_table))
            .transpose()
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "profile trace width".into(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Route has no explicit width and PDK has no 'trace.min_width' constraint".into(),
                hint: "Add 'width: <value>' to the route, or declare 'trace: min_width: <value>' in the profile.".into(),
            })?
    };

    // Resolve material from stackup layer at the start pin's Z position
    let start_pin_z = space.netlist.get_pin_position(start_pin_id)
        .map(|pos| pos.2)
        .unwrap_or(0);
    let copper_id = (|| -> Option<hwc_engine::material::MaterialId> {
        let layer_name = stackup_manager.get_layer_name_at_z(start_pin_z)?;
        let mat_name = profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup.layers.iter()
                    .find(|l| l.name.name == layer_name)
                    .map(|l| l.material.clone())
            })?;
        space.material_registry.get_id(&mat_name)
    })()
    .ok_or_else(|| IrError::UndeclaredMaterial {
        material: format!("No material found at Z={}nm (check stackup definition)", start_pin_z).into(),
    })?;

    // Check if start pin already has a net
    let existing_net = if let Some(pin_data) = space.netlist.get_pin(start_pin_id) {
        pin_data.connected_net
    } else {
        None
    };

    let goal_net = if let Some(pin_data) = space.netlist.get_pin(goal_pin_id) {
        pin_data.connected_net
    } else {
        None
    };

    let net_id = match (existing_net, goal_net) {
        // Both endpoints already on the same net → good, use it
        (Some(e), Some(g)) if e == g => e,
        
        // Both endpoints have different nets → this is a SHORT CIRCUIT error!
        // Do NOT merge them automatically - report the conflict
        (Some(e), Some(g)) => {
            let e_name = space.netlist.get_net(e).map(|n| n.name.as_str()).unwrap_or("unknown");
            let g_name = space.netlist.get_net(g).map(|n| n.name.as_str()).unwrap_or("unknown");
            
            // If both are semantic nets (not auto-generated), this is a user error
            if !e_name.starts_with("NET_") && !g_name.starts_with("NET_") && e_name != g_name {
                return Err(IrError::InvalidRouteExpression {
                    expression: format!("route {} to {}", 
                        construct_entity_name(&route.from)?,
                        construct_entity_name(&route.to)?),
                    reason: format!(
                        "Route would short-circuit two different nets: '{}' and '{}'. \
                        Endpoints are already connected to different nets.",
                        e_name, g_name
                    ),
                });
            }
            
            // One is auto-generated → merge into the semantic net
            let (keep, drop) = if e_name.starts_with("NET_") && !g_name.starts_with("NET_") {
                (g, e)
            } else if !e_name.starts_with("NET_") && g_name.starts_with("NET_") {
                (e, g)
            } else {
                // Both auto-generated or same name → merge (shouldn't happen but handle it)
                (e, g)
            };
            
            // Only merge if 'drop' is an auto-generated net
            let drop_name = space.netlist.get_net(drop).map(|n| n.name.as_str()).unwrap_or("");
            if drop_name.starts_with("NET_") {
                if let Some(drop_pins) = space.netlist.get_net_pins(drop).map(|p| p.to_vec()) {
                    for p in drop_pins {
                        space.netlist.connect_pin(p, keep);
                    }
                }
            }
            keep
        },
        
        // One endpoint has a net, the other doesn't → use the existing net
        (Some(e), None) => e,
        (None, Some(g)) => g,
        
        // Neither endpoint has a net → create a new one
        (None, None) => space.netlist.add_net(net_name.clone(), width_nm, copper_id),
    };

    // Connect both pins to the net in the logical netlist
    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    // Get the actual net name (may be different from the generated one if it already existed)
    let actual_net_name = space
        .netlist
        .get_net(net_id)
        .map(|n| n.name.clone())
        .unwrap_or(net_name);

    // Handshake B - Synchronize netlist metadata
    // ONLY update entity net assignments if we created a new net or merged nets.
    // Do NOT update if both endpoints already had the correct net - this prevents
    // accidental net ID reassignment due to get_or_create_net returning a new ID.
    if existing_net.is_none() || goal_net.is_none() || existing_net != goal_net {
        let start_name = construct_entity_name(&route.from)?;
        let goal_name = construct_entity_name(&route.to)?;

        space
            .entity_graph
            .set_entity_net(&start_name, actual_net_name.as_str());
        space
            .entity_graph
            .set_entity_net(&goal_name, actual_net_name.as_str());
    }

    Ok(net_id)
}


