//! Routing module for trace routing between pins.

mod automatic;
pub mod electrical_optimizer;
mod global;
pub(crate) mod helpers;
mod manual;
pub mod types;

pub use automatic::{calculate_boundary_points, route_automatic};
pub use global::AutoRouter;
pub use helpers::{
    evaluate_index_expression, get_pin_positions, needs_automatic_routing, register_net_for_route,
    resolve_endpoint_entity_ids, resolve_route_boundary_points, resolve_route_pin_centers,
};
pub use manual::route_manual;
pub use types::{CardinalDirection, EdgeOffset, EscapeSpec, ResolvedRoute};

use super::errors::IrError;
use hwc_engine::HardwareSpace;

/// Route a trace between pins.
///
/// Automatically selects between automatic topological routing or manual waypoint routing
/// based on whether waypoints are provided.
pub fn route_trace(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext, // UNIVERSAL CONTEXT
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    if needs_automatic_routing(route) {
        route_automatic(space, route, symbol_table, stackup_manager, profile)
    } else {
        route_manual(
            space,
            route,
            origin,
            symbol_table,
            eval_context,
            stackup_manager,
            profile,
        )
    }
}

/// v0.1.8: Convert a `PatternInstantiation` (AST) into a `RoutingPattern` (engine).
///
/// Looks up the pattern definition by name, binds instantiation arguments to
/// definition parameters, evaluates each step's distance/angle expressions,
/// and returns an engine-ready `RoutingPattern`.
pub fn instantiate_pattern(
    instantiation: &hwc_parser::PatternInstantiation,
    symbol_table: &crate::SymbolTable,
) -> Result<hwc_engine::RoutingPattern, IrError> {
    use hwc_engine::{PatternStep, RoutingPattern};

    let definition = symbol_table.get_pattern(&instantiation.name).map_err(|e| {
        IrError::InvalidRouteExpression {
            expression: format!("pattern '{}'", instantiation.name),
            reason: format!("Unknown pattern '{}': {}", instantiation.name, e),
        }
    })?;

    // Build evaluation context: bind instantiation args to definition param names
    let mut eval_ctx: rustc_hash::FxHashMap<compact_str::CompactString, f64> =
        rustc_hash::FxHashMap::default();
    for arg in &instantiation.arguments {
        let val_nm = crate::ir::conversions::evaluate_expression_to_nm(&arg.value, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: format!("pattern argument '{}'", arg.name),
                reason: format!("Failed to evaluate pattern argument '{}': {}", arg.name, e),
            })?;
        eval_ctx.insert(arg.name.clone(), val_nm as f64);
    }

    // Evaluate each step's distance and angle using the local context
    let mut steps = Vec::with_capacity(definition.steps.len());
    for step in &definition.steps {
        let dist_nm = evaluate_with_ctx(&step.distance, &eval_ctx, symbol_table)?;
        let angle_deg = evaluate_with_ctx(&step.angle, &eval_ctx, symbol_table)?;
        steps.push(PatternStep {
            distance_nm: dist_nm,
            angle_deg,
        });
    }

    Ok(RoutingPattern::new(instantiation.name.clone(), steps))
}

/// Evaluate an expression using a local variable context (pattern arguments)
/// with fallback to the symbol table for constants.
fn evaluate_with_ctx(
    expr: &hwc_parser::Expression,
    ctx: &rustc_hash::FxHashMap<compact_str::CompactString, f64>,
    symbol_table: &crate::SymbolTable,
) -> Result<i64, IrError> {
    match expr {
        hwc_parser::Expression::Literal { value, .. } => Ok(*value),
        hwc_parser::Expression::FloatLiteral { value, .. } => Ok(*value as i64),
        hwc_parser::Expression::Measurement { value, unit, .. } => match unit {
            hwc_parser::Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
            hwc_parser::Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
            hwc_parser::Unit::Micrometer => Ok((value * 1_000.0) as i64),
            hwc_parser::Unit::Nanometer => Ok(*value as i64),
            _ => Err(IrError::InvalidRouteExpression {
                expression: format!("{:?}", unit),
                reason: format!("Cannot convert {:?} to nanometers", unit),
            }),
        },
        hwc_parser::Expression::Variable { name, .. } => {
            // First check local context (pattern arguments), then symbol table
            if let Some(val) = ctx.get(name.as_str()) {
                Ok(*val as i64)
            } else if let Some(val) = symbol_table.get_all_constants().get(name) {
                Ok(*val as i64)
            } else {
                Err(IrError::InvalidRouteExpression {
                    expression: format!("variable '{}'", name),
                    reason: format!("Unknown variable: '{}'", name),
                })
            }
        }
        hwc_parser::Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let l = evaluate_with_ctx(left, ctx, symbol_table)?;
            let r = evaluate_with_ctx(right, ctx, symbol_table)?;
            match operator {
                hwc_parser::BinaryOperator::Add => Ok(l + r),
                hwc_parser::BinaryOperator::Subtract => Ok(l - r),
                hwc_parser::BinaryOperator::Multiply => Ok(l * r),
                hwc_parser::BinaryOperator::Divide => {
                    if r == 0 {
                        Err(IrError::InvalidRouteExpression {
                            expression: "division".into(),
                            reason: "Division by zero".into(),
                        })
                    } else {
                        Ok(l / r)
                    }
                }
                hwc_parser::BinaryOperator::Modulo => Ok(l % r),
            }
        }
        hwc_parser::Expression::Unary {
            operator, operand, ..
        } => {
            let val = evaluate_with_ctx(operand, ctx, symbol_table)?;
            match operator {
                hwc_parser::UnaryOperator::Negate => Ok(-val),
                hwc_parser::UnaryOperator::Plus => Ok(val),
            }
        }
        hwc_parser::Expression::Grouped { expression, .. } => {
            evaluate_with_ctx(expression, ctx, symbol_table)
        }
        _ => Err(IrError::InvalidRouteExpression {
            expression: "pattern step".into(),
            reason: "Unsupported expression in pattern step".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_automatic_routing_with_path() {
        let route = hwc_parser::Route {
            from: hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name: "Power".into(),
                component_index: None,
                pin_name: "Plus".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            to: hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name: "Light".into(),
                component_index: None,
                pin_name: "Anode".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            width: None,
            layer: None,
            strategy: None,
            pattern: None,
            strategy_params: vec![],
            path: Some(vec![hwc_parser::Coordinate::Positional {
                x: hwc_parser::Expression::Literal {
                    value: 1,
                    span: hwc_parser::Span::new(0, 1),
                },
                y: hwc_parser::Expression::Literal {
                    value: 15,
                    span: hwc_parser::Span::new(0, 2),
                },
                z: hwc_parser::Expression::Literal {
                    value: 15,
                    span: hwc_parser::Span::new(0, 2),
                },
                span: hwc_parser::Span::new(0, 0),
            }]),
            signal_group: None,
            bridge: None,
            enter_escape: None,
            exit_escape: None,
            current_limit_ac: None,
            span: hwc_parser::Span::new(0, 0),
        };

        assert!(!needs_automatic_routing(&route));
    }

    #[test]
    fn test_needs_automatic_routing_without_path() {
        let route = hwc_parser::Route {
            from: hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name: "Power".into(),
                component_index: None,
                pin_name: "Plus".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            to: hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name: "Light".into(),
                component_index: None,
                pin_name: "Anode".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            width: None,
            layer: None,
            strategy: None,
            pattern: None,
            strategy_params: vec![],
            path: None,
            signal_group: None,
            bridge: None,
            enter_escape: None,
            exit_escape: None,
            current_limit_ac: None,
            span: hwc_parser::Span::new(0, 0),
        };

        assert!(needs_automatic_routing(&route));
    }
}
