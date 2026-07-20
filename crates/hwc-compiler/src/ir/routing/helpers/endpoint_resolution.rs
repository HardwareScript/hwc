use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::HardwareSpace;

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

/// Resolve EntityIds for route endpoints from the EntityGraph (v0.1.9).
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

                Ok(hwc_engine::geometry::EntityId::from_semantic(&format!(
                    "pin:{}:{}",
                    full_comp_name, full_pin_name
                )))
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

/// Generate a list of available routing endpoints for error messages.
pub(crate) fn list_available_endpoints(space: &HardwareSpace) -> String {
    let mut endpoints = Vec::new();

    let entity_count = space.entity_graph.iter_entity_ids().count();
    eprintln!(
        "[DEBUG list_available_endpoints] Entity count at error construction: {}",
        entity_count
    );

    for entity_id in space.entity_graph.iter_entity_ids() {
        if let Ok(entity_data) = space.entity_graph.get_entity_data(*entity_id) {
            let name = entity_data.name.as_str();
            eprintln!(
                "[DEBUG] Entity name: {} (type: {:?})",
                name, entity_data.entity_type
            );

            match entity_data.entity_type {
                hwc_engine::geometry_router::entity_graph::EntityType::ComponentPin => {
                    if let Some((comp_name, pin_name)) = name.split_once(':') {
                        endpoints.push(format!("{}.{}", comp_name, pin_name));
                    } else {
                        endpoints.push(name.to_string());
                    }
                }
                hwc_engine::geometry_router::entity_graph::EntityType::SpacePour => {
                    endpoints.push(name.to_string());
                }
                _ => {}
            }
        }
    }

    eprintln!(
        "[DEBUG] Parsed {} endpoints from {} entities",
        endpoints.len(),
        entity_count
    );

    if endpoints.is_empty() {
        "\n\nAvailable endpoints: (none registered yet)".to_string()
    } else {
        endpoints.sort();
        endpoints.dedup();
        format!("\n\nAvailable endpoints:\n  {}", endpoints.join("\n  "))
    }
}
