use crate::ir::errors::IrError;
use compact_str::CompactString;

/// Human-readable label for a route endpoint.
pub fn endpoint_label(endpoint: &hwc_parser::RouteEndpointSpec) -> String {
    construct_entity_name(endpoint)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Construct full entity name from route endpoint (v0.1.8).
pub fn construct_entity_name(
    endpoint: &hwc_parser::RouteEndpointSpec,
) -> Result<CompactString, IrError> {
    match endpoint {
        hwc_parser::RouteEndpointSpec::ComponentPin {
            component_name,
            component_index,
            pin_name,
            ..
        } => {
            // v0.2.1: Check if this is actually a hierarchical space reference
            // Format: "InstanceName.EntityName" where EntityName is the space pour/pad
            // This gets parsed as ComponentPin but should be treated as SpaceEntity
            let full_name = if let Some(ref index_expr) = component_index {
                let _index_value = evaluate_index_expression(index_expr)?;
                format!("{}", component_name)
            } else {
                format!("{}.{}", component_name, pin_name)
            };
            Ok(full_name.into())
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

/// Evaluate an index expression to a concrete integer value.
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
                // Comparison operators return 1 for true, 0 for false
                BinaryOperator::Equal => Ok(if left_val == right_val { 1 } else { 0 }),
                BinaryOperator::NotEqual => Ok(if left_val != right_val { 1 } else { 0 }),
                BinaryOperator::LessThan => Ok(if left_val < right_val { 1 } else { 0 }),
                BinaryOperator::GreaterThan => Ok(if left_val > right_val { 1 } else { 0 }),
                BinaryOperator::LessThanOrEqual => Ok(if left_val <= right_val { 1 } else { 0 }),
                BinaryOperator::GreaterThanOrEqual => Ok(if left_val >= right_val { 1 } else { 0 }),
                // Boolean operators (treat non-zero as true)
                BinaryOperator::And => Ok(if left_val != 0 && right_val != 0 {
                    1
                } else {
                    0
                }),
                BinaryOperator::Or => Ok(if left_val != 0 || right_val != 0 {
                    1
                } else {
                    0
                }),
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
                UnaryOperator::Not => Ok(if operand_val == 0 { 1 } else { 0 }),
            }
        }
        hwc_parser::Expression::Grouped { expression, .. } => evaluate_index_expression(expression),
        _ => Err(IrError::InvalidExpression(format!(
            "Cannot evaluate non-arithmetic expression as index: {:?}",
            expr
        ))),
    }
}

/// Resolve EntityIds for route endpoints from the EntityGraph (v0.1.9 / v0.2.1).
pub fn resolve_endpoint_entity_ids(
    route: &hwc_parser::Route,
) -> Result<
    (
        hwc_engine::geometry::EntityId,
        hwc_engine::geometry::EntityId,
    ),
    IrError,
> {
    let resolve_entity_id = |endpoint: &hwc_parser::RouteEndpointSpec| -> Result<hwc_engine::geometry::EntityId, IrError> {
        match endpoint {
            hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name,
                component_index,
                pin_name,
                pin_index,
                ..
            } => {
                let full_comp_name = if let Some(ref idx_expr) = component_index {
                    let idx_val = evaluate_index_expression(idx_expr)?;
                    format!("{}[{}]", component_name, idx_val)
                } else {
                    component_name.to_string()
                };

                let full_pin_name = if let Some(ref idx_expr) = pin_index {
                    let idx_val = evaluate_index_expression(idx_expr)?;
                    format!("{}[{}]", pin_name, idx_val)
                } else {
                    pin_name.to_string()
                };

                // v0.2.1: Hierarchical cross-instance routing fix.
                //
                // The parser emits "route PMOS_Inst.Out_Pad to NMOS_Inst.Out_Pad" as
                // ComponentPin { component_name: "PMOS_Inst", pin_name: "Out_Pad" }.
                //
                // However, transform_entity_registry() registers the flattened child
                // entity under the key "space:PMOS_Inst.Out_Pad" (SpacePour type),
                // NOT "pin:PMOS_Inst:Out_Pad".
                //
                // So when the component has no index expression and there is a non-empty
                // pin_name, interpret this as a hierarchical space-entity reference:
                //   "space:InstanceName.EntityName"
                //
                // This is correct because:
                //   - Real intra-space pin routes use `pin_name` that refers to a local
                //     pin (e.g. ComponentPin inside child space), not a space-entity.
                //   - Cross-instance parent routes use ComponentPin syntax but target
                //     entities registered as SpacePour in the parent EntityGraph.
                if component_index.is_none() && !full_pin_name.is_empty() {
                    // Treat as "space:InstanceName.EntityName"
                    Ok(hwc_engine::geometry::EntityId::from_semantic(&format!(
                        "space:{}.{}",
                        full_comp_name, full_pin_name
                    )))
                } else {
                    Ok(hwc_engine::geometry::EntityId::from_semantic(&format!(
                        "pin:{}:{}",
                        full_comp_name, full_pin_name
                    )))
                }
            }
            hwc_parser::RouteEndpointSpec::SpaceEntity { name, index, .. } => {
                let full_name = if let Some(ref idx_expr) = index {
                    let idx_val = evaluate_index_expression(idx_expr)?;
                    format!("{}[{}]", name, idx_val)
                } else {
                    name.to_string()
                };

                Ok(hwc_engine::geometry::EntityId::from_semantic(&format!(
                    "space:{}",
                    full_name
                )))
            }
        }
    };

    let from_id = resolve_entity_id(&route.from)?;
    let to_id = resolve_entity_id(&route.to)?;

    Ok((from_id, to_id))
}
