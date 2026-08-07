use super::super::expression::evaluate_anchor_index_expression;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_parser::Expression;

pub fn substitute_in_anchor_name(
    anchor_name: &str,
    variable: &str,
    value: usize,
) -> Result<CompactString, IrError> {
    if let Some(open_bracket) = anchor_name.find('[') {
        if let Some(close_bracket) = anchor_name.rfind(']') {
            let base_name = &anchor_name[..open_bracket];
            let index_str = &anchor_name[open_bracket + 1..close_bracket];
            let evaluated_index = evaluate_anchor_index_expression(index_str, variable, value)?;
            return Ok(format!("{}[{}]", base_name, evaluated_index).into());
        }
    }
    Ok(anchor_name.into())
}

pub fn substitute_in_expression(
    expr: &Expression,
    variable: &str,
    value: usize,
) -> Result<Expression, IrError> {
    match expr {
        Expression::Literal { value: v, span } => Ok(Expression::Literal {
            value: *v,
            span: *span,
        }),
        Expression::FloatLiteral { value: v, span } => Ok(Expression::FloatLiteral {
            value: *v,
            span: *span,
        }),
        Expression::Measurement {
            value: v,
            unit,
            span,
        } => Ok(Expression::Measurement {
            value: *v,
            unit: unit.clone(),
            span: *span,
        }),
        Expression::Percentage { value: v, span } => Ok(Expression::Percentage {
            value: *v,
            span: *span,
        }),
        Expression::Variable { name, span } => {
            if name == variable {
                Ok(Expression::Literal {
                    value: value as i64,
                    span: *span,
                })
            } else {
                Ok(Expression::Variable {
                    name: name.clone(),
                    span: *span,
                })
            }
        }
        Expression::Binary {
            left,
            operator,
            right,
            span,
        } => {
            let left_sub = substitute_in_expression(left, variable, value)?;
            let right_sub = substitute_in_expression(right, variable, value)?;
            Ok(Expression::Binary {
                left: Box::new(left_sub),
                operator: *operator,
                right: Box::new(right_sub),
                span: *span,
            })
        }
        Expression::Unary {
            operator,
            operand,
            span,
        } => {
            let operand_sub = substitute_in_expression(operand, variable, value)?;
            Ok(Expression::Unary {
                operator: *operator,
                operand: Box::new(operand_sub),
                span: *span,
            })
        }
        Expression::Grouped { expression, span } => {
            let expression_sub = substitute_in_expression(expression, variable, value)?;
            Ok(Expression::Grouped {
                expression: Box::new(expression_sub),
                span: *span,
            })
        }
        Expression::AnchorReference { anchor, edge, span } => {
            let anchor_name = if anchor.name == "last" {
                "last".into()
            } else {
                substitute_in_anchor_name(&anchor.name, variable, value)?
            };
            Ok(Expression::AnchorReference {
                anchor: hwc_parser::AnchorReference {
                    name: anchor_name,
                    span: anchor.span,
                },
                edge: *edge,
                span: *span,
            })
        }
        Expression::Coordinate { coord, span } => {
            let coord_sub = substitute_in_coordinate(coord, variable, value)?;
            Ok(Expression::Coordinate {
                coord: Box::new(coord_sub),
                span: *span,
            })
        }
        Expression::FunctionCall {
            name,
            arguments,
            span,
        } => {
            // Substitute in all arguments
            let mut substituted_args = Vec::new();
            for arg in arguments {
                substituted_args.push(substitute_in_expression(arg, variable, value)?);
            }
            Ok(Expression::FunctionCall {
                name: name.clone(),
                arguments: substituted_args,
                span: *span,
            })
        }
    }
}

pub fn substitute_in_coordinate(
    coord: &hwc_parser::Coordinate,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::Coordinate, IrError> {
    match coord {
        hwc_parser::Coordinate::Positional { x, y, z, span } => {
            let x_sub = substitute_in_expression(x, variable, value)?;
            let y_sub = substitute_in_expression(y, variable, value)?;
            let z_sub = substitute_in_expression(z, variable, value)?;
            Ok(hwc_parser::Coordinate::Positional {
                x: x_sub,
                y: y_sub,
                z: z_sub,
                span: *span,
            })
        }
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            let x_sub = substitute_in_expression(x, variable, value)?;
            let y_sub = substitute_in_expression(y, variable, value)?;
            let z_sub = substitute_in_expression(z, variable, value)?;
            Ok(hwc_parser::Coordinate::Declarative {
                x: x_sub,
                y: y_sub,
                z: z_sub,
                span: *span,
            })
        }
        hwc_parser::Coordinate::Relative(rel_pos) => {
            let anchor_name = if rel_pos.anchor.name == "last" {
                "last".into()
            } else {
                substitute_in_anchor_name(&rel_pos.anchor.name, variable, value)?
            };

            let anchor = hwc_parser::AnchorReference {
                name: anchor_name,
                span: rel_pos.anchor.span,
            };

            let offset = match &rel_pos.offset {
                hwc_parser::RelativeOffset::Single(measurement) => {
                    hwc_parser::RelativeOffset::Single(measurement.clone())
                }
                hwc_parser::RelativeOffset::Vector { x, y, z } => {
                    let x_sub = substitute_in_expression(x, variable, value)?;
                    let y_sub = substitute_in_expression(y, variable, value)?;
                    let z_sub = substitute_in_expression(z, variable, value)?;
                    hwc_parser::RelativeOffset::Vector {
                        x: x_sub,
                        y: y_sub,
                        z: z_sub,
                    }
                }
            };

            Ok(hwc_parser::Coordinate::Relative(
                hwc_parser::RelativePosition {
                    anchor,
                    edge: rel_pos.edge,
                    offset,
                    span: rel_pos.span,
                },
            ))
        }
    }
}

pub fn substitute_in_route_endpoint(
    endpoint: &hwc_parser::RouteEndpointSpec,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::RouteEndpointSpec, IrError> {
    match endpoint {
        hwc_parser::RouteEndpointSpec::ComponentPin {
            component_name,
            component_index,
            pin_name,
            pin_index,
            span,
        } => {
            let component_index_sub = if let Some(ref expr) = component_index {
                Some(substitute_in_expression(expr, variable, value)?)
            } else {
                None
            };
            let pin_index_sub = if let Some(ref expr) = pin_index {
                Some(substitute_in_expression(expr, variable, value)?)
            } else {
                None
            };
            Ok(hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name: component_name.clone(),
                component_index: component_index_sub,
                pin_name: pin_name.clone(),
                pin_index: pin_index_sub,
                span: *span,
            })
        }
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, index, span } => {
            let index_sub = if let Some(ref expr) = index {
                Some(substitute_in_expression(expr, variable, value)?)
            } else {
                None
            };
            Ok(hwc_parser::RouteEndpointSpec::SpaceEntity {
                name: name.clone(),
                index: index_sub,
                span: *span,
            })
        }
    }
}
