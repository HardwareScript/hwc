//! Coordinate expression evaluation utilities.
//!
//! Fast path for evaluating coordinate expressions directly to nanometers
//! without constructing intermediate AST nodes.

use super::super::errors::IrError;
use crate::SymbolTable;

/// Evaluate a coordinate expression to nanometers (optimized for internal pour unrolling).
pub fn evaluate_coordinate_to_nm(
    expr: &hwc_parser::Expression,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<i64, IrError> {
    use hwc_parser::{BinaryOperator, Expression, UnaryOperator, Unit};

    match expr {
        Expression::Literal { value, .. } => Ok(*value),
        Expression::FloatLiteral { value, .. } => Ok(*value as i64),
        Expression::Measurement { value, unit, .. } => {
            let nm = match unit {
                Unit::Millimeter => {
                    let pm = (value * 1_000_000_000.0).round() as i64;
                    pm / 1000
                }
                Unit::Centimeter => {
                    let pm = (value * 10_000_000_000.0).round() as i64;
                    pm / 1000
                }
                Unit::Micrometer => {
                    let pm = (value * 1_000_000.0).round() as i64;
                    pm / 1000
                }
                Unit::Nanometer => *value as i64,
                Unit::Custom(symbol) => {
                    if let Some(unit_def) = symbol_table.resolve_unit_symbol(symbol) {
                        let multiplier = unit_def.multiplier.unwrap_or(1.0);
                        let pm = (value * multiplier * 1_000_000_000_000.0).round() as i64;
                        pm / 1000
                    } else {
                        return Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("unit symbol '{}'", symbol),
                            reason: format!("Unknown unit symbol: '{}'", symbol),
                        });
                    }
                }
                _ => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("{:?}", unit),
                        reason: format!("Cannot convert {:?} to nanometers", unit),
                    })
                }
            };
            Ok(nm)
        }
        Expression::Variable { name, .. } => {
            if let Some(value) = eval_context.get(name) {
                // Convert the strongly-typed Value to nanometers
                value
                    .to_nanometers()
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("variable '{}'", name),
                        reason: e,
                    })
            } else if let Some(const_value) = symbol_table.get_all_constants().get(name) {
                Ok(*const_value as i64)
            } else {
                Err(IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("constant '{}'", name),
                    reason: format!("Unknown constant: '{}'", name),
                })
            }
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let left_nm = evaluate_coordinate_to_nm(left, symbol_table, eval_context)?;
            let right_nm = evaluate_coordinate_to_nm(right, symbol_table, eval_context)?;
            let result = match operator {
                BinaryOperator::Add => left_nm + right_nm,
                BinaryOperator::Subtract => left_nm - right_nm,
                BinaryOperator::Multiply => left_nm * right_nm,
                BinaryOperator::Divide => {
                    if right_nm == 0 {
                        return Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: "division".into(),
                            reason: "Division by zero".into(),
                        });
                    }
                    left_nm / right_nm
                }
                BinaryOperator::Modulo => left_nm % right_nm,
                // Comparison operators return 1 for true, 0 for false
                BinaryOperator::Equal => {
                    if left_nm == right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::NotEqual => {
                    if left_nm != right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::LessThan => {
                    if left_nm < right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::GreaterThan => {
                    if left_nm > right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::LessThanOrEqual => {
                    if left_nm <= right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::GreaterThanOrEqual => {
                    if left_nm >= right_nm {
                        1
                    } else {
                        0
                    }
                }
                // Boolean operators (treat non-zero as true)
                BinaryOperator::And => {
                    if left_nm != 0 && right_nm != 0 {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::Or => {
                    if left_nm != 0 || right_nm != 0 {
                        1
                    } else {
                        0
                    }
                }
            };
            Ok(result)
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_nm = evaluate_coordinate_to_nm(operand, symbol_table, eval_context)?;
            let result = match operator {
                UnaryOperator::Negate => -operand_nm,
                UnaryOperator::Plus => operand_nm,
                UnaryOperator::Not => {
                    if operand_nm == 0 {
                        1
                    } else {
                        0
                    }
                }
            };
            Ok(result)
        }
        Expression::Grouped { expression, .. } => {
            evaluate_coordinate_to_nm(expression, symbol_table, eval_context)
        }
        Expression::Percentage { .. } => Err(IrError::CoordinateResolutionFailed {
            coordinate_str: "percentage expression".into(),
            reason: "Percentages not supported here".into(),
        }),
        Expression::AnchorReference { .. } => Err(IrError::CoordinateResolutionFailed {
            coordinate_str: "anchor reference".into(),
            reason: "Anchor references require evaluate_coordinate_with_anchors".into(),
        }),
        Expression::Coordinate { .. } => Err(IrError::CoordinateResolutionFailed {
            coordinate_str: "coordinate literal".into(),
            reason: "Coordinate literals cannot be evaluated to a single nanometer value".into(),
        }),
        Expression::FunctionCall { .. } => {
            // Function calls need to be evaluated through the normal expression evaluator
            // which has full context for variable resolution
            let value =
                expr.evaluate(eval_context)
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: "function call".into(),
                        reason: e,
                    })?;
            value
                .to_nanometers()
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: "function call result".into(),
                    reason: e,
                })
        }
    }
}

/// Evaluate a measurement to nanometers using the proper unit conversion system
pub fn evaluate_measurement_to_nm(
    measurement: &hwc_parser::Measurement,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<i64, IrError> {
    let expr = hwc_parser::Expression::Measurement {
        value: measurement.value,
        unit: measurement.unit.clone(),
        span: measurement.span,
    };
    evaluate_coordinate_to_nm(&expr, symbol_table, eval_context)
}

/// Axis context for coordinate evaluation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateAxis {
    X,
    Y,
    Z,
}

/// Evaluate a coordinate expression that may contain anchor references.
pub fn evaluate_coordinate_with_anchors(
    expr: &hwc_parser::Expression,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    bbox_tracker: &crate::bounding_box_tracker::BoundingBoxTracker,
    context_axis: CoordinateAxis,
    origin_z: hwc_parser::OriginZ,
) -> Result<i64, IrError> {
    use hwc_parser::{BinaryOperator, Expression, UnaryOperator};

    match expr {
        Expression::AnchorReference { anchor, edge, .. } => {
            let resolved_anchor_name = if anchor.name == "last" {
                bbox_tracker
                    .last_registered()
                    .ok_or_else(|| IrError::CoordinateResolutionFailed {
                        coordinate_str: "anchor 'last'".into(),
                        reason: "No 'last' component found".into(),
                    })?
                    .clone()
            } else {
                anchor.name.clone()
            };

            let anchor_bbox = bbox_tracker.get(&resolved_anchor_name).ok_or_else(|| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("anchor '{}'", resolved_anchor_name),
                    reason: format!("Anchor '{}' not found", resolved_anchor_name),
                }
            })?;

            let engine_edge = match edge {
                hwc_parser::Edge::Left
                | hwc_parser::Edge::TopLeft
                | hwc_parser::Edge::BottomLeft => hwc_engine::geometry::Edge::Left,
                hwc_parser::Edge::Right
                | hwc_parser::Edge::TopRight
                | hwc_parser::Edge::BottomRight => hwc_engine::geometry::Edge::Right,
                hwc_parser::Edge::Top => hwc_engine::geometry::Edge::Top,
                hwc_parser::Edge::Bottom => hwc_engine::geometry::Edge::Bottom,
                hwc_parser::Edge::Front => hwc_engine::geometry::Edge::Front,
                hwc_parser::Edge::Back => hwc_engine::geometry::Edge::Back,
                hwc_parser::Edge::MinZ => hwc_engine::geometry::Edge::MinZ,
                hwc_parser::Edge::MaxZ => hwc_engine::geometry::Edge::MaxZ,
                hwc_parser::Edge::Center => hwc_engine::geometry::Edge::Left,
                hwc_parser::Edge::CenterX => hwc_engine::geometry::Edge::Left,
                hwc_parser::Edge::CenterY => hwc_engine::geometry::Edge::Left,
                hwc_parser::Edge::CenterZ => hwc_engine::geometry::Edge::Left,
            };

            let edge_point = anchor_bbox.edge_point(engine_edge);

            let coord_nm = match edge {
                hwc_parser::Edge::Left | hwc_parser::Edge::Right => edge_point.x,
                hwc_parser::Edge::Top | hwc_parser::Edge::Bottom => {
                    match context_axis {
                        CoordinateAxis::Z => {
                            // Sprint 6: last.top and last.bottom for Z-axis
                            // Result depends on OriginZ (Top-Down vs Bottom-Up)
                            match origin_z {
                                hwc_parser::OriginZ::Bottom => {
                                    // Layer indices increase with height (1=Ground, 10=Sky)
                                    // Top is max, Bottom is min
                                    if *edge == hwc_parser::Edge::Top {
                                        anchor_bbox.max.z
                                    } else {
                                        anchor_bbox.min.z
                                    }
                                }
                                hwc_parser::OriginZ::Top => {
                                    // Layer indices increase with depth (1=Sky, 10=Ground)
                                    // Top is min, Bottom is max
                                    if *edge == hwc_parser::Edge::Top {
                                        anchor_bbox.min.z
                                    } else {
                                        anchor_bbox.max.z
                                    }
                                }
                            }
                        }
                        _ => edge_point.y,
                    }
                }
                hwc_parser::Edge::TopLeft | hwc_parser::Edge::BottomLeft => match context_axis {
                    CoordinateAxis::X => anchor_bbox.min.x,
                    CoordinateAxis::Y => {
                        if *edge == hwc_parser::Edge::TopLeft {
                            anchor_bbox.max.y
                        } else {
                            anchor_bbox.min.y
                        }
                    }
                    CoordinateAxis::Z => anchor_bbox.min.z,
                },
                hwc_parser::Edge::TopRight | hwc_parser::Edge::BottomRight => match context_axis {
                    CoordinateAxis::X => anchor_bbox.max.x,
                    CoordinateAxis::Y => {
                        if *edge == hwc_parser::Edge::TopRight {
                            anchor_bbox.max.y
                        } else {
                            anchor_bbox.min.y
                        }
                    }
                    CoordinateAxis::Z => anchor_bbox.min.z,
                },
                hwc_parser::Edge::Center => match context_axis {
                    CoordinateAxis::X => (anchor_bbox.min.x + anchor_bbox.max.x) / 2,
                    CoordinateAxis::Y => (anchor_bbox.min.y + anchor_bbox.max.y) / 2,
                    CoordinateAxis::Z => (anchor_bbox.min.z + anchor_bbox.max.z) / 2,
                },
                hwc_parser::Edge::CenterX => match context_axis {
                    CoordinateAxis::X => (anchor_bbox.min.x + anchor_bbox.max.x) / 2,
                    CoordinateAxis::Y => anchor_bbox.min.y,
                    CoordinateAxis::Z => anchor_bbox.min.z,
                },
                hwc_parser::Edge::CenterY => match context_axis {
                    CoordinateAxis::X => anchor_bbox.min.x,
                    CoordinateAxis::Y => (anchor_bbox.min.y + anchor_bbox.max.y) / 2,
                    CoordinateAxis::Z => anchor_bbox.min.z,
                },
                hwc_parser::Edge::CenterZ => match context_axis {
                    CoordinateAxis::X => anchor_bbox.min.x,
                    CoordinateAxis::Y => anchor_bbox.min.y,
                    CoordinateAxis::Z => (anchor_bbox.min.z + anchor_bbox.max.z) / 2,
                },
                hwc_parser::Edge::Front
                | hwc_parser::Edge::Back
                | hwc_parser::Edge::MinZ
                | hwc_parser::Edge::MaxZ => edge_point.z,
            };

            Ok(coord_nm)
        }

        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let left_nm = evaluate_coordinate_with_anchors(
                left,
                symbol_table,
                eval_context,
                bbox_tracker,
                context_axis,
                origin_z,
            )?;
            let right_nm = evaluate_coordinate_with_anchors(
                right,
                symbol_table,
                eval_context,
                bbox_tracker,
                context_axis,
                origin_z,
            )?;
            let result = match operator {
                BinaryOperator::Add => left_nm + right_nm,
                BinaryOperator::Subtract => left_nm - right_nm,
                BinaryOperator::Multiply => left_nm * right_nm,
                BinaryOperator::Divide => {
                    if right_nm == 0 {
                        return Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: "division".into(),
                            reason: "Division by zero".into(),
                        });
                    }
                    left_nm / right_nm
                }
                BinaryOperator::Modulo => left_nm % right_nm,
                // Comparison operators return 1 for true, 0 for false
                BinaryOperator::Equal => {
                    if left_nm == right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::NotEqual => {
                    if left_nm != right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::LessThan => {
                    if left_nm < right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::GreaterThan => {
                    if left_nm > right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::LessThanOrEqual => {
                    if left_nm <= right_nm {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::GreaterThanOrEqual => {
                    if left_nm >= right_nm {
                        1
                    } else {
                        0
                    }
                }
                // Boolean operators (treat non-zero as true)
                BinaryOperator::And => {
                    if left_nm != 0 && right_nm != 0 {
                        1
                    } else {
                        0
                    }
                }
                BinaryOperator::Or => {
                    if left_nm != 0 || right_nm != 0 {
                        1
                    } else {
                        0
                    }
                }
            };
            Ok(result)
        }

        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_nm = evaluate_coordinate_with_anchors(
                operand,
                symbol_table,
                eval_context,
                bbox_tracker,
                context_axis,
                origin_z,
            )?;
            let result = match operator {
                UnaryOperator::Negate => -operand_nm,
                UnaryOperator::Plus => operand_nm,
                UnaryOperator::Not => {
                    if operand_nm == 0 {
                        1
                    } else {
                        0
                    }
                }
            };
            Ok(result)
        }

        Expression::Grouped { expression, .. } => evaluate_coordinate_with_anchors(
            expression,
            symbol_table,
            eval_context,
            bbox_tracker,
            context_axis,
            origin_z,
        ),

        _ => evaluate_coordinate_to_nm(expr, symbol_table, eval_context),
    }
}
