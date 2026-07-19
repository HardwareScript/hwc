//! Relational constraint resolver (v0.1.9)
//!
//! Resolves middle-level relational placement constraints (align, above, below,
//! right_of, left_of) into absolute coordinates using the BoundingBoxTracker.
//!
//! This runs BEFORE the main placement loop, converting relational descriptions
//! into concrete positions that the existing placement pipeline can process.

use compact_str::CompactString;
use hwc_parser::{
    AlignmentAxis, ComponentName, Coordinate, DirectionalConstraint, Expression,
    RelationalConstraint, Unit,
};

use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::ir::errors::IrError;

/// Resolve relational constraints for all components in the placement list.
///
/// For each component with relational constraints but no explicit position,
/// this computes the absolute position from the constraints and sets it.
///
/// Components must be processed in dependency order (topological sort ensures
/// this) so that referenced targets have their bounding boxes available.
pub fn resolve_relational_constraints(
    placement_items: &mut Vec<crate::ir::PlacementItem>,
    bbox_tracker: &BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    origin: hwc_parser::OriginPoint,
) -> Result<(), IrError> {
    for item in placement_items.iter_mut() {
        match item {
            crate::ir::PlacementItem::Component(component) => {
                if component.position.is_some() || component.relational_constraints.is_empty() {
                    continue; // Already has position or no constraints
                }

                let resolved = compute_position_from_constraints(
                    &component.relational_constraints,
                    &component.name,
                    bbox_tracker,
                    symbol_table,
                    eval_context,
                    origin,
                )?;

                component.position = Some(resolved);
            }
            crate::ir::PlacementItem::Plane(plane) => {
                // v0.1.9: Handle relational constraints for planes
                if plane.from.is_some() || plane.relational_constraints.is_empty() {
                    continue; // Already has position or no constraints
                }

                let resolved = compute_position_from_constraints(
                    &plane.relational_constraints,
                    &Some(plane.name.clone()),
                    bbox_tracker,
                    symbol_table,
                    eval_context,
                    origin,
                )?;

                plane.from = Some(resolved);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Compute an absolute position from relational constraints.
///
/// Evaluates alignment and directional constraints against the BoundingBoxTracker
/// to produce a concrete `Coordinate::Declarative` position.
///
/// **ORIGIN-AWARE COORDINATE SYSTEM**: This function dynamically adapts to the
/// space's declared origin (TL, BL, TR, BR) by computing axis-direction multipliers.
/// This ensures that directional operators (above, below, right_of, left_of) work
/// correctly regardless of whether Y increases upward (BL) or downward (TL).
///
/// **CENTER ALIGNMENT SEMANTICS**: For center alignment (center_x, center_y, center_z),
/// this function returns the CENTER coordinate of the target object. The caller is
/// responsible for computing the object's dimensions and adjusting the position to
/// achieve center-to-center alignment.
///
/// This two-phase approach is necessary because:
/// 1. Shape dimensions are resolved separately in the placement pipeline
/// 2. The coordinate resolution phase doesn't have access to geometric properties
/// 3. This matches the document spec: "The compiler's constraint manager evaluates
///    the relative layout instructions" [Spatial_Synthesis_Abstraction.md §1.4.1]
pub fn compute_position_from_constraints(
    constraints: &[RelationalConstraint],
    _component_name: &Option<ComponentName>,
    bbox_tracker: &BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    _eval_context: &hwc_parser::EvaluationContext,
    origin: hwc_parser::OriginPoint,
) -> Result<Coordinate, IrError> {
    // Derive axis-direction multipliers from the declared origin
    // This ensures physical directions (UP, DOWN, LEFT, RIGHT) map correctly
    // to coordinate deltas regardless of the coordinate system orientation
    let (x_multiplier, y_multiplier) = match origin.xy {
        hwc_parser::OriginXY::BL => (1, 1),   // Bottom-Left: +X right, +Y up
        hwc_parser::OriginXY::TL => (1, -1),  // Top-Left: +X right, +Y down
        hwc_parser::OriginXY::BR => (-1, 1),  // Bottom-Right: +X left, +Y up
        hwc_parser::OriginXY::TR => (-1, -1), // Top-Right: +X left, +Y down
    };

    let mut x_nm: Option<i64> = None;
    let mut y_nm: Option<i64> = None;
    let mut z_nm: Option<i64> = None;

    for constraint in constraints {
        match constraint {
            RelationalConstraint::Align { axis, target, .. } => {
                let target_bbox = resolve_target_bbox(target, bbox_tracker)?;

                match axis {
                    AlignmentAxis::CenterX => {
                        let center = (target_bbox.min.x + target_bbox.max.x) / 2;
                        x_nm = Some(center);
                    }
                    AlignmentAxis::CenterY => {
                        let center = (target_bbox.min.y + target_bbox.max.y) / 2;
                        y_nm = Some(center);
                    }
                    AlignmentAxis::CenterZ => {
                        let center = (target_bbox.min.z + target_bbox.max.z) / 2;
                        z_nm = Some(center);
                    }
                    AlignmentAxis::Top => {
                        // Align to top edge of target (min Y in screen coords)
                        y_nm = Some(target_bbox.min.y);
                    }
                    AlignmentAxis::Bottom => {
                        // Align to bottom edge of target (max Y in screen coords)
                        y_nm = Some(target_bbox.max.y);
                    }
                    AlignmentAxis::Left => {
                        // Align to left edge of target
                        x_nm = Some(target_bbox.min.x);
                    }
                    AlignmentAxis::Right => {
                        // Align to right edge of target
                        x_nm = Some(target_bbox.max.x);
                    }
                }
            }
            RelationalConstraint::Directional(dir) => {
                let (target, spacing_expr) = match dir {
                    DirectionalConstraint::Above { target, spacing } => (target, spacing),
                    DirectionalConstraint::Below { target, spacing } => (target, spacing),
                    DirectionalConstraint::RightOf { target, spacing } => (target, spacing),
                    DirectionalConstraint::LeftOf { target, spacing } => (target, spacing),
                };

                let target_bbox = resolve_target_bbox(target, bbox_tracker)?;
                let spacing_nm = if let Some(expr) = spacing_expr {
                    evaluate_expression_to_nm(expr, symbol_table)?
                } else {
                    0
                };

                match dir {
                    DirectionalConstraint::Above { .. } => {
                        // Physical UP: Move away from ground in the physical world
                        // In BL (y_multiplier=1): UP means +Y → target.max.y + spacing
                        // In TL (y_multiplier=-1): UP means -Y → target.min.y - spacing
                        if y_multiplier > 0 {
                            // Bottom-Left or Bottom-Right: Y increases upward
                            y_nm = Some(target_bbox.max.y + spacing_nm);
                        } else {
                            // Top-Left or Top-Right: Y decreases upward (toward origin)
                            y_nm = Some(target_bbox.min.y - spacing_nm);
                        }
                        // Inherit X-center alignment from target to preserve vertical stacking
                        if x_nm.is_none() {
                            x_nm = Some((target_bbox.min.x + target_bbox.max.x) / 2);
                        }
                    }
                    DirectionalConstraint::Below { .. } => {
                        // Physical DOWN: Move toward ground in the physical world
                        // In BL (y_multiplier=1): DOWN means -Y → target.min.y - spacing
                        // In TL (y_multiplier=-1): DOWN means +Y → target.max.y + spacing
                        if y_multiplier > 0 {
                            // Bottom-Left or Bottom-Right: Y decreases downward
                            y_nm = Some(target_bbox.min.y - spacing_nm);
                        } else {
                            // Top-Left or Top-Right: Y increases downward (away from origin)
                            y_nm = Some(target_bbox.max.y + spacing_nm);
                        }
                        // Inherit X-center alignment from target to preserve vertical stacking
                        if x_nm.is_none() {
                            x_nm = Some((target_bbox.min.x + target_bbox.max.x) / 2);
                        }
                    }
                    DirectionalConstraint::RightOf { .. } => {
                        // Physical RIGHT: Move to the right in the physical world
                        // In BL/TL (x_multiplier=1): RIGHT means +X → target.max.x + spacing
                        // In BR/TR (x_multiplier=-1): RIGHT means -X → target.min.x - spacing
                        if x_multiplier > 0 {
                            // Left-origin: X increases to the right
                            x_nm = Some(target_bbox.max.x + spacing_nm);
                        } else {
                            // Right-origin: X decreases to the right (toward origin)
                            x_nm = Some(target_bbox.min.x - spacing_nm);
                        }
                        // Inherit Y-center alignment from target to preserve horizontal alignment
                        if y_nm.is_none() {
                            y_nm = Some((target_bbox.min.y + target_bbox.max.y) / 2);
                        }
                    }
                    DirectionalConstraint::LeftOf { .. } => {
                        // Physical LEFT: Move to the left in the physical world
                        // In BL/TL (x_multiplier=1): LEFT means -X → target.min.x - spacing
                        // In BR/TR (x_multiplier=-1): LEFT means +X → target.max.x + spacing
                        if x_multiplier > 0 {
                            // Left-origin: X decreases to the left
                            x_nm = Some(target_bbox.min.x - spacing_nm);
                        } else {
                            // Right-origin: X increases to the left (away from origin)
                            x_nm = Some(target_bbox.max.x + spacing_nm);
                        }
                        // Inherit Y-center alignment from target to preserve horizontal alignment
                        if y_nm.is_none() {
                            y_nm = Some((target_bbox.min.y + target_bbox.max.y) / 2);
                        }
                    }
                }
            }
        }
    }

    // Build the coordinate from resolved values
    let x = x_nm.unwrap_or(0);
    let y = y_nm.unwrap_or(0);
    let z = z_nm.unwrap_or(0);

    Ok(Coordinate::Declarative {
        x: Expression::Measurement {
            value: x as f64,
            unit: Unit::Nanometer,
            span: hwc_parser::Span::new(0, 0),
        },
        y: Expression::Measurement {
            value: y as f64,
            unit: Unit::Nanometer,
            span: hwc_parser::Span::new(0, 0),
        },
        z: Expression::Measurement {
            value: z as f64,
            unit: Unit::Nanometer,
            span: hwc_parser::Span::new(0, 0),
        },
        span: hwc_parser::Span::new(0, 0),
    })
}

/// Resolve a target component name to its bounding box from the tracker.
fn resolve_target_bbox(
    target: &ComponentName,
    bbox_tracker: &BoundingBoxTracker,
) -> Result<hwc_engine::geometry::BoundingBox, IrError> {
    let target_name: CompactString = target.base.clone();

    bbox_tracker
        .get(&target_name)
        .cloned()
        .ok_or_else(|| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("target '{}'", target_name),
            reason: format!(
                "Target component '{}' not found in bounding box tracker. \
                 Ensure it is placed before components that reference it.",
                target_name
            ),
        })
}

/// Evaluate an expression to nanometers (simplified for constraint resolution).
fn evaluate_expression_to_nm(
    expr: &Expression,
    symbol_table: &crate::SymbolTable,
) -> Result<i64, IrError> {
    match expr {
        Expression::Literal { value, .. } => Ok(*value),
        Expression::FloatLiteral { value, .. } => Ok(*value as i64),
        Expression::Measurement { value, unit, .. } => {
            let nm = match unit {
                Unit::Millimeter => (*value * 1_000_000.0).round() as i64,
                Unit::Centimeter => (*value * 10_000_000.0).round() as i64,
                Unit::Micrometer => (*value * 1_000.0).round() as i64,
                Unit::Nanometer => *value as i64,
                Unit::Picometer => (*value / 1000.0).round() as i64,
                Unit::Custom(symbol) => {
                    if let Some(unit_def) = symbol_table.resolve_unit_symbol(symbol) {
                        let multiplier = unit_def.multiplier.unwrap_or(1.0);
                        (*value * multiplier * 1_000.0).round() as i64
                    } else {
                        return Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("unit '{}'", symbol),
                            reason: format!("Unknown unit: '{}'", symbol),
                        });
                    }
                }
                _ => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("{:?}", unit),
                        reason: format!("Cannot convert {:?} to nanometers", unit),
                    });
                }
            };
            Ok(nm)
        }
        Expression::Variable { name, .. } => {
            if let Some(value) = symbol_table.get_all_constants().get(name) {
                Ok(*value as i64)
            } else {
                Err(IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("variable '{}'", name),
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
            let left_nm = evaluate_expression_to_nm(left, symbol_table)?;
            let right_nm = evaluate_expression_to_nm(right, symbol_table)?;
            match operator {
                hwc_parser::BinaryOperator::Add => Ok(left_nm + right_nm),
                hwc_parser::BinaryOperator::Subtract => Ok(left_nm - right_nm),
                hwc_parser::BinaryOperator::Multiply => Ok(left_nm * right_nm),
                hwc_parser::BinaryOperator::Divide => {
                    if right_nm == 0 {
                        Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: "division".into(),
                            reason: "Division by zero".into(),
                        })
                    } else {
                        Ok(left_nm / right_nm)
                    }
                }
                hwc_parser::BinaryOperator::Modulo => Ok(left_nm % right_nm),
            }
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_nm = evaluate_expression_to_nm(operand, symbol_table)?;
            match operator {
                hwc_parser::UnaryOperator::Negate => Ok(-operand_nm),
                hwc_parser::UnaryOperator::Plus => Ok(operand_nm),
            }
        }
        Expression::Grouped { expression, .. } => {
            evaluate_expression_to_nm(expression, symbol_table)
        }
        _ => Err(IrError::CoordinateResolutionFailed {
            coordinate_str: "expression".into(),
            reason: "Unsupported expression type in relational constraint".into(),
        }),
    }
}
