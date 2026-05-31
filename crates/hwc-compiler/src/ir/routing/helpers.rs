//! Shared routing utilities.

use super::super::errors::IrError;
use compact_str::CompactString;
use hwc_engine::{HardwareSpace, Point3D};
use rustc_hash::FxHashMap;

/// Construct full component name from pin reference (Sprint 3.10: Parametric Routing)
///
/// Handles component array indices:
/// - `Adder` + None → `"Adder"`
/// - `Adder` + Some(Expression::Literal { value: 0, .. }) → `"Adder[0]"`
/// - `Adder` + Some(Expression::Binary { 0 + 1 }) → `"Adder[1]"` (evaluates expression)
pub fn construct_component_name(
    pin_ref: &hwc_parser::PinReference,
) -> Result<CompactString, IrError> {
    if let Some(ref index_expr) = pin_ref.component_index {
        // Evaluate the expression to get a concrete index
        let index_value = evaluate_index_expression(index_expr)?;
        Ok(format!("{}[{}]", pin_ref.component, index_value).into())
    } else {
        Ok(pin_ref.component.clone())
    }
}

/// Evaluate an index expression to a concrete integer value
///
/// Handles:
/// - Literals: `0`, `1`, `5`
/// - Binary operations: `0 + 1` → `1`, `2 * 3` → `6`
fn evaluate_index_expression(expr: &hwc_parser::Expression) -> Result<i64, IrError> {
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

/// Get start and goal positions from pin references.
pub fn get_pin_positions(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(Point3D, Point3D), IrError> {
    let (start_id, goal_id) = get_pin_ids(space, route)?;
    
    let start_pos_tuple = space.netlist.get_pin_position(start_id).ok_or_else(|| IrError::PinNotFound {
        component: route.from.component.clone(),
        pin: route.from.pin.to_string(),
    })?;
    let start_pos = Point3D::new(start_pos_tuple.0, start_pos_tuple.1, start_pos_tuple.2);

    let goal_pos_tuple = space.netlist.get_pin_position(goal_id).ok_or_else(|| IrError::PinNotFound {
        component: route.to.component.clone(),
        pin: route.to.pin.to_string(),
    })?;
    let goal_pos = Point3D::new(goal_pos_tuple.0, goal_pos_tuple.1, goal_pos_tuple.2);

    Ok((start_pos, goal_pos))
}

/// Get start and goal pin IDs from pin references.
pub fn get_pin_ids(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(hwc_engine::netlist::PinId, hwc_engine::netlist::PinId), IrError> {
    // Construct full component names including array indices if present
    let start_component_name = construct_component_name(&route.from)?;
    let goal_component_name = construct_component_name(&route.to)?;

    let start_component_id = space
        .netlist
        .get_component_by_name(&start_component_name)
        .ok_or_else(|| IrError::PinNotFound {
            component: start_component_name.clone(),
            pin: route.from.pin.to_string(),
        })?;

    let goal_component_id = space
        .netlist
        .get_component_by_name(&goal_component_name)
        .ok_or_else(|| IrError::PinNotFound {
            component: goal_component_name.clone(),
            pin: route.to.pin.to_string(),
        })?;

    let start_pin_name = if let Some(ref index_expr) = route.from.pin_index {
        match index_expr {
            hwc_parser::Expression::Literal { value, .. } => {
                format!("{}[{}]", route.from.pin, value)
            }
            _ => route.from.pin.to_string(),
        }
    } else {
        route.from.pin.to_string()
    };

    let goal_pin_name = if let Some(ref index_expr) = route.to.pin_index {
        match index_expr {
            hwc_parser::Expression::Literal { value, .. } => format!("{}[{}]", route.to.pin, value),
            _ => route.to.pin.to_string(),
        }
    } else {
        route.to.pin.to_string()
    };

    let start_pins = space.netlist.get_component_pins(start_component_id);
    let start_pin_id = start_pins
        .iter()
        .find(|&&pin_id| {
            if let Some(pin_data) = space.netlist.get_pin(pin_id) {
                pin_data.name == start_pin_name
            } else {
                false
            }
        })
        .ok_or_else(|| IrError::PinNotFound {
            component: route.from.component.clone(),
            pin: start_pin_name.clone(),
        })?;

    let goal_pins = space.netlist.get_component_pins(goal_component_id);
    let goal_pin_id = goal_pins
        .iter()
        .find(|&&pin_id| {
            if let Some(pin_data) = space.netlist.get_pin(pin_id) {
                pin_data.name == goal_pin_name
            } else {
                false
            }
        })
        .ok_or_else(|| IrError::PinNotFound {
            component: route.to.component.clone(),
            pin: goal_pin_name.clone(),
        })?;

    Ok((*start_pin_id, *goal_pin_id))
}

/// Register a net for a route and connect the source and target pins.
///
/// This ensures the netlist reflects the connectivity intent, allowing
/// the global router to discover and realize the route.
pub fn register_net_for_route(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
) -> Result<hwc_engine::netlist::NetId, IrError> {
    let (start_pin_id, goal_pin_id) = get_pin_ids(space, route)?;

    // Construct a unique net name if none provided
    let net_name: CompactString = format!(
        "NET_{}_{}_to_{}_{}",
        route.from.component, route.from.pin, route.to.component, route.to.pin
    )
    .into();

    // Get trace width and material
    let width_nm = if let Some(w_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(w_expr, symbol_table)
            .map_err(IrError::RoutingError)?
    } else {
        100_000 // Default 100um
    };

    let copper_id = space.material_registry.get_or_register("Copper");

    // Check if start pin already has a net
    let existing_net = if let Some(pin_data) = space.netlist.get_pin(start_pin_id) {
        pin_data.connected_net
    } else {
        None
    };

    let net_id = if let Some(id) = existing_net {
        id
    } else {
        space.netlist.add_net(net_name.clone(), width_nm, copper_id)
    };

    // Connect both pins to the net in the logical netlist
    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    // Get the actual net name (may be different from the generated one if it already existed)
    let actual_net_name = space.netlist.get_net(net_id).map(|n| n.name.clone()).unwrap_or(net_name);

    // v0.1.7: Handshake B - Synchronize VoxelGrid metadata
    // This ensures the AutoRouter can find these pins when analyzing nets.
    let start_pin_name = space.netlist.get_pin(start_pin_id).unwrap().name.clone();
    let goal_pin_name = space.netlist.get_pin(goal_pin_id).unwrap().name.clone();
    let start_comp_name = construct_component_name(&route.from)?;
    let goal_comp_name = construct_component_name(&route.to)?;

    space.voxel_grid.set_pin_net(&start_comp_name, &start_pin_name, actual_net_name.as_str());
    space.voxel_grid.set_pin_net(&goal_comp_name, &goal_pin_name, actual_net_name.as_str());

    Ok(net_id)
}

/// Collect all existing nets from the voxel grid for DRC validation.
pub fn collect_existing_nets(
    space: &HardwareSpace,
) -> Vec<hwc_engine::design_rule_check::NetVoxels> {
    use hwc_engine::design_rule_check::NetVoxels;

    eprintln!(
        "[ROUTER DEBUG] collect_existing_nets: Using SPARSE iteration over occupied voxels..."
    );

    let mut nets_map: FxHashMap<u32, Vec<Point3D>> = FxHashMap::default();

    // CRITICAL FIX: Use sparse iteration instead of dense O(N³) loop
    // This iterates only over occupied chunks, not all 4 billion voxel coordinates!
    for (x, y, z, _material, net_handle) in space.voxel_grid.iter_occupied() {
        let net_id = net_handle.0;
        if net_id > 0 {
            let position = space.voxel_to_position(x, y, z);
            nets_map.entry(net_id).or_default().push(position);
        }
    }

    eprintln!(
        "[ROUTER DEBUG] Sparse iteration complete. Found {} nets",
        nets_map.len()
    );

    nets_map
        .into_iter()
        .map(|(net_id, voxels)| {
            let net_name = space
                .netlist
                .get_net(hwc_engine::netlist::NetId::new(net_id))
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Net_{}", net_id).into());

            let classification = space
                .net_classifications
                .get(&net_name)
                .copied()
                .unwrap_or(hwc_engine::space::NetClassification::Unclassified);

            NetVoxels {
                net_name,
                voxels,
                geometry_type: hwc_engine::design_rule_check::GeometryType::Trace, // Routed nets are traces
                classification,
            }
        })
        .collect()
}
