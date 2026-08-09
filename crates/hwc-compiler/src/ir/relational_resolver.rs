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

/// Unified spatial relation (directional or alignment)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialRelation {
    // Directional
    Above,
    Below,
    LeftOf,
    RightOf,
    // Alignment
    AlignTop,
    AlignBottom,
    AlignLeft,
    AlignRight,
    AlignX,
    AlignY,
}

/// A formula for calculating the user-coordinate offset.
///
/// Contains all parameters needed to compute the position along the axis.
pub struct RelationalPlacementFormula {
    /// True if the relationship acts on the Y axis, false if X axis
    pub is_y_axis: bool,
    /// Which target edge to reference (Min or Max user coordinate)
    /// Represented as a boolean: true for max, false for min.
    pub use_target_max: bool,
    /// Multiplier for the spacing (1, -1, or 0)
    pub spacing_multiplier: i64,
    /// Multiplier for the new item's own dimension (1, -1, or 0)
    pub self_dimension_multiplier: i64,
    /// True if this is center alignment (requires special center-to-center offset)
    pub is_center_alignment: bool,
}

impl RelationalPlacementFormula {
    /// Get the formula parameters for a given relation and origin multipliers
    pub fn get(relation: SpatialRelation, x_multiplier: i64, y_multiplier: i64) -> Self {
        match relation {
            SpatialRelation::RightOf => {
                if x_multiplier > 0 {
                    // BL/TL: +X right. target_max.x + spacing
                    Self {
                        is_y_axis: false,
                        use_target_max: true,
                        spacing_multiplier: 1,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                } else {
                    // BR/TR: -X right. target_min.x - spacing
                    Self {
                        is_y_axis: false,
                        use_target_max: false,
                        spacing_multiplier: -1,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::LeftOf => {
                if x_multiplier > 0 {
                    // BL/TL: -X left. target_min.x - spacing - self_width
                    Self {
                        is_y_axis: false,
                        use_target_max: false,
                        spacing_multiplier: -1,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                } else {
                    // BR/TR: +X left. target_max.x + spacing
                    Self {
                        is_y_axis: false,
                        use_target_max: true,
                        spacing_multiplier: 1,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::Above => {
                if y_multiplier > 0 {
                    // BL/BR: +Y up. target_max.y + spacing
                    Self {
                        is_y_axis: true,
                        use_target_max: true,
                        spacing_multiplier: 1,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                } else {
                    // TL/TR: -Y up. target_min.y - spacing - self_height
                    Self {
                        is_y_axis: true,
                        use_target_max: false,
                        spacing_multiplier: -1,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::Below => {
                if y_multiplier > 0 {
                    // BL/BR: -Y down. target_min.y - spacing - self_height
                    Self {
                        is_y_axis: true,
                        use_target_max: false,
                        spacing_multiplier: -1,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                } else {
                    // TL/TR: +Y down. target_max.y + spacing
                    Self {
                        is_y_axis: true,
                        use_target_max: true,
                        spacing_multiplier: 1,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::AlignTop => {
                if y_multiplier > 0 {
                    // BL/BR: physical top is user max. A's user Y + height = B's user max
                    Self {
                        is_y_axis: true,
                        use_target_max: true,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                } else {
                    // TL/TR: physical top is user min. A's user Y = B's user min
                    Self {
                        is_y_axis: true,
                        use_target_max: false,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::AlignBottom => {
                if y_multiplier > 0 {
                    // BL/BR: physical bottom is user min. A's user Y = B's user min
                    Self {
                        is_y_axis: true,
                        use_target_max: false,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                } else {
                    // TL/TR: physical bottom is user max. A's user Y + height = B's user max
                    Self {
                        is_y_axis: true,
                        use_target_max: true,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::AlignLeft => {
                if x_multiplier > 0 {
                    // BL/TL: physical left is user min. A's user X = B's user min
                    Self {
                        is_y_axis: false,
                        use_target_max: false,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                } else {
                    // BR/TR: physical left is user max. A's user X + width = B's user max
                    Self {
                        is_y_axis: false,
                        use_target_max: true,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::AlignRight => {
                if x_multiplier > 0 {
                    // BL/TL: physical right is user max. A's user X + width = B's user max
                    Self {
                        is_y_axis: false,
                        use_target_max: true,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: -1,
                        is_center_alignment: false,
                    }
                } else {
                    // BR/TR: physical right is user min. A's user X = B's user min
                    Self {
                        is_y_axis: false,
                        use_target_max: false,
                        spacing_multiplier: 0,
                        self_dimension_multiplier: 0,
                        is_center_alignment: false,
                    }
                }
            }
            SpatialRelation::AlignX => Self {
                is_y_axis: false,
                use_target_max: false,
                spacing_multiplier: 0,
                self_dimension_multiplier: 0,
                is_center_alignment: true,
            },
            SpatialRelation::AlignY => Self {
                is_y_axis: true,
                use_target_max: false,
                spacing_multiplier: 0,
                self_dimension_multiplier: 0,
                is_center_alignment: true,
            },
        }
    }

    /// Resolve the position using the formula
    pub fn resolve(&self, t_min: i64, t_max: i64, spacing_nm: i64, self_dimension_nm: i64) -> i64 {
        if self.is_center_alignment {
            let center = (t_min + t_max) / 2;
            center - (self_dimension_nm / 2)
        } else {
            let base_edge = if self.use_target_max { t_max } else { t_min };
            base_edge
                + (self.spacing_multiplier * spacing_nm)
                + (self.self_dimension_multiplier * self_dimension_nm)
        }
    }
}

/// Convert physical BoundingBox coordinates to User Space coordinates.
///
/// v0.2.1: Under the canonical Bottom-Left origin, user space and physical
/// space are identical, so this is an identity mapping. It is retained as a
/// named seam so callers document intent at the conversion boundary.
pub fn target_bbox_to_user_ranges(
    target_bbox: &hwc_engine::geometry::BoundingBox,
    _space_dimensions: &hwc_engine::Dimensions,
) -> (i64, i64, i64, i64) {
    (
        target_bbox.min.x,
        target_bbox.max.x,
        target_bbox.min.y,
        target_bbox.max.y,
    )
}

/// Resolve relational constraints for all components in the placement list.
///
/// Removed in v0.2.x: relational constraints are now resolved inline in
/// `compilation::placement_loop::execute_placement`, which already walks items
/// in topological order and has the arena available to read each node in place.
/// The old signature took `&mut [PlacementItem]`, which is impossible now that
/// placement items are `Copy` arena IDs rather than owned AST nodes.
/// Compute an absolute position from relational constraints.
///
/// Evaluates alignment and directional constraints against the BoundingBoxTracker
/// to produce a concrete `Coordinate::Declarative` position.
///
/// **CANONICAL COORDINATE SYSTEM**: v0.2.1 purged the `origin:` declaration.
/// Every space is Bottom-Left / Z-Up, so +X is rightward and +Y is upward.
/// Directional operators (above, below, right_of, left_of) map directly onto
/// positive/negative coordinate deltas with no axis inversion.
///
/// **IMPORTANT**: This function returns the EXACT target coordinate:
/// - For center alignment: Returns the center point of the target
/// - For edge alignment: Returns the edge coordinate
/// - For directional placement: Returns the computed offset position
///
/// The CALLER (plane.rs/component placement) is responsible for dimension-aware
/// adjustments (e.g., subtracting half-width for center_x alignment).
pub fn compute_position_from_constraints(
    constraints: &[RelationalConstraint],
    _component_name: &Option<ComponentName>,
    bbox_tracker: &BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    space_dimensions: &hwc_engine::Dimensions,
) -> Result<Coordinate, IrError> {
    // v0.2.1: Canonical Bottom-Left origin — +X is rightward and +Y is upward,
    // so physical directions map to coordinate deltas with no inversion.
    let (x_multiplier, y_multiplier) = (1i64, 1i64);

    let mut x_nm: Option<i64> = None;
    let mut y_nm: Option<i64> = None;
    let mut z_nm: Option<i64> = None;

    for constraint in constraints {
        match constraint {
            RelationalConstraint::Align { axis, target, .. } => {
                // v0.2.1: Handle both entity targets and expression targets
                let resolved_value_nm = match target {
                    hwc_parser::AlignmentTarget::Entity(component_name) => {
                        // Traditional entity-based alignment
                        let target_bbox = resolve_target_bbox(component_name, bbox_tracker)?;
                        let (tx_min, tx_max, ty_min, ty_max) =
                            target_bbox_to_user_ranges(&target_bbox, space_dimensions);

                        // Return the appropriate coordinate based on axis
                        match axis {
                            AlignmentAxis::Center => {
                                // Center aligns BOTH X and Y
                                x_nm = Some((tx_min + tx_max) / 2);
                                y_nm = Some((ty_min + ty_max) / 2);
                                return Ok(Coordinate::Declarative {
                                    x: Expression::Measurement {
                                        value: x_nm.unwrap() as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    y: Expression::Measurement {
                                        value: y_nm.unwrap() as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    z: Expression::Measurement {
                                        value: z_nm.unwrap_or(0) as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    span: hwc_parser::Span::new(0, 0),
                                });
                            }
                            AlignmentAxis::X => (tx_min + tx_max) / 2,
                            AlignmentAxis::Y => (ty_min + ty_max) / 2,
                            AlignmentAxis::Z => (target_bbox.min.z + target_bbox.max.z) / 2,
                            AlignmentAxis::Top => {
                                let formula = RelationalPlacementFormula::get(
                                    SpatialRelation::AlignTop,
                                    x_multiplier,
                                    y_multiplier,
                                );
                                formula.resolve(ty_min, ty_max, 0, 0)
                            }
                            AlignmentAxis::Bottom => {
                                let formula = RelationalPlacementFormula::get(
                                    SpatialRelation::AlignBottom,
                                    x_multiplier,
                                    y_multiplier,
                                );
                                formula.resolve(ty_min, ty_max, 0, 0)
                            }
                            AlignmentAxis::Left => {
                                let formula = RelationalPlacementFormula::get(
                                    SpatialRelation::AlignLeft,
                                    x_multiplier,
                                    y_multiplier,
                                );
                                formula.resolve(tx_min, tx_max, 0, 0)
                            }
                            AlignmentAxis::Right => {
                                let formula = RelationalPlacementFormula::get(
                                    SpatialRelation::AlignRight,
                                    x_multiplier,
                                    y_multiplier,
                                );
                                formula.resolve(tx_min, tx_max, 0, 0)
                            }
                        }
                    }
                    hwc_parser::AlignmentTarget::Expression(expr) => {
                        // v0.2.1: Expression-based alignment - evaluate the expression
                        // The expression should evaluate to a coordinate value (e.g., (A.center_x + B.center_x) / 2)
                        use crate::ir::placement::coordinate_evaluation::{
                            evaluate_coordinate_with_anchors, CoordinateAxis,
                        };

                        let context_axis = match axis {
                            AlignmentAxis::Center => {
                                return Ok(Coordinate::Declarative {
                                    x: Expression::Measurement {
                                        value: x_nm.unwrap() as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    y: Expression::Measurement {
                                        value: y_nm.unwrap() as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    z: Expression::Measurement {
                                        value: z_nm.unwrap_or(0) as f64,
                                        unit: Unit::Nanometer,
                                        span: hwc_parser::Span::new(0, 0),
                                    },
                                    span: hwc_parser::Span::new(0, 0),
                                })
                            } // Already handled above
                            AlignmentAxis::X | AlignmentAxis::Left | AlignmentAxis::Right => {
                                CoordinateAxis::X
                            }
                            AlignmentAxis::Y | AlignmentAxis::Top | AlignmentAxis::Bottom => {
                                CoordinateAxis::Y
                            }
                            AlignmentAxis::Z => CoordinateAxis::Z,
                        };

                        evaluate_coordinate_with_anchors(
                            expr,
                            symbol_table,
                            eval_context,
                            bbox_tracker,
                            context_axis,
                        )?
                    }
                };

                // Assign to the appropriate axis
                match axis {
                    AlignmentAxis::Center => unreachable!("Center handled earlier"),
                    AlignmentAxis::X | AlignmentAxis::Left | AlignmentAxis::Right => {
                        x_nm = Some(resolved_value_nm);
                    }
                    AlignmentAxis::Y | AlignmentAxis::Top | AlignmentAxis::Bottom => {
                        y_nm = Some(resolved_value_nm);
                    }
                    AlignmentAxis::Z => {
                        z_nm = Some(resolved_value_nm);
                    }
                }
            }
            RelationalConstraint::Directional(dir) => {
                let (target, spacing_expr, relation) = match dir {
                    DirectionalConstraint::Above { target, spacing } => {
                        (target, spacing, SpatialRelation::Above)
                    }
                    DirectionalConstraint::Below { target, spacing } => {
                        (target, spacing, SpatialRelation::Below)
                    }
                    DirectionalConstraint::RightOf { target, spacing } => {
                        (target, spacing, SpatialRelation::RightOf)
                    }
                    DirectionalConstraint::LeftOf { target, spacing } => {
                        (target, spacing, SpatialRelation::LeftOf)
                    }
                };

                let target_bbox = resolve_target_bbox(target, bbox_tracker)?;
                let (tx_min, tx_max, ty_min, ty_max) =
                    target_bbox_to_user_ranges(&target_bbox, space_dimensions);
                let spacing_nm = if let Some(expr) = spacing_expr {
                    evaluate_expression_to_nm(expr, symbol_table)?
                } else {
                    0
                };

                let formula = RelationalPlacementFormula::get(relation, x_multiplier, y_multiplier);
                if formula.is_y_axis {
                    y_nm = Some(formula.resolve(ty_min, ty_max, spacing_nm, 0));
                    if x_nm.is_none() {
                        x_nm = Some((tx_min + tx_max) / 2);
                    }
                } else {
                    x_nm = Some(formula.resolve(tx_min, tx_max, spacing_nm, 0));
                    if y_nm.is_none() {
                        y_nm = Some((ty_min + ty_max) / 2);
                    }
                }
            }
        }
    }

    // Build the coordinate from resolved values
    // v0.2.0: Fail loudly if constraints don't resolve - no silent fallbacks!
    let x = x_nm.ok_or_else(|| IrError::CoordinateResolutionFailed {
        coordinate_str: "X coordinate".into(),
        reason: "No relational constraint resolved the X coordinate".into(),
    })?;
    let y = y_nm.ok_or_else(|| IrError::CoordinateResolutionFailed {
        coordinate_str: "Y coordinate".into(),
        reason: "No relational constraint resolved the Y coordinate".into(),
    })?;
    let z = z_nm.unwrap_or(0); // Z is optional (defaults to layer bottom)

    eprintln!(
        "[RELATIONAL_RESOLVER] Resolved position: X={}nm, Y={}nm, Z={}nm",
        x, y, z
    );

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

    eprintln!(
        "[RELATIONAL_RESOLVER DEBUG] Resolving target: '{}'",
        target_name
    );
    eprintln!(
        "[RELATIONAL_RESOLVER DEBUG] Available entities in bbox_tracker: {:?}",
        bbox_tracker.all_names()
    );

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
                // Comparison operators return 1 for true, 0 for false
                hwc_parser::BinaryOperator::Equal => Ok(if left_nm == right_nm { 1 } else { 0 }),
                hwc_parser::BinaryOperator::NotEqual => Ok(if left_nm != right_nm { 1 } else { 0 }),
                hwc_parser::BinaryOperator::LessThan => Ok(if left_nm < right_nm { 1 } else { 0 }),
                hwc_parser::BinaryOperator::GreaterThan => {
                    Ok(if left_nm > right_nm { 1 } else { 0 })
                }
                hwc_parser::BinaryOperator::LessThanOrEqual => {
                    Ok(if left_nm <= right_nm { 1 } else { 0 })
                }
                hwc_parser::BinaryOperator::GreaterThanOrEqual => {
                    Ok(if left_nm >= right_nm { 1 } else { 0 })
                }
                // Boolean operators (treat non-zero as true)
                hwc_parser::BinaryOperator::And => {
                    Ok(if left_nm != 0 && right_nm != 0 { 1 } else { 0 })
                }
                hwc_parser::BinaryOperator::Or => {
                    Ok(if left_nm != 0 || right_nm != 0 { 1 } else { 0 })
                }
            }
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_nm = evaluate_expression_to_nm(operand, symbol_table)?;
            match operator {
                hwc_parser::UnaryOperator::Negate => Ok(-operand_nm),
                hwc_parser::UnaryOperator::Plus => Ok(operand_nm),
                hwc_parser::UnaryOperator::Not => Ok(if operand_nm == 0 { 1 } else { 0 }),
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
