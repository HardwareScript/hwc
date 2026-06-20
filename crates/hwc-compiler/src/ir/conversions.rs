//! Unit conversions and coordinate transformations.

use crate::ir::placement::coordinate_evaluation::CoordinateAxis;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::{GridCells, Point3D, VoxelSize};
use hwc_parser::{Coordinate, Expression, Measurement, Unit, Value};

/// Context for coordinate-to-point conversion operations.
/// Groups related parameters to avoid exceeding Clippy's argument limit.
pub struct CoordinateContext<'a> {
    pub voxel_size: &'a VoxelSize,
    pub grid_size: &'a GridCells,
    pub origin: hwc_parser::OriginPoint,
    pub space_dimensions: &'a hwc_engine::Dimensions,
    pub symbol_table: &'a crate::SymbolTable,
    pub eval_context: &'a hwc_parser::EvaluationContext,
    pub bbox_tracker: Option<&'a crate::bounding_box_tracker::BoundingBoxTracker>,
    pub stackup_manager: &'a StackupManager,
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
}

/// GAP 2 FIX: Smart Expression Evaluator with Unit Normalization
pub fn evaluate_expression_to_nm(
    expr: &Expression,
    symbol_table: &crate::SymbolTable,
) -> Result<i64, String> {
    match expr {
        Expression::Literal { value, .. } => Ok(*value),
        Expression::FloatLiteral { value, .. } => Ok(*value as i64),
        Expression::Measurement { value, unit, .. } => match unit {
            Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
            Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
            Unit::Micrometer => Ok((value * 1_000.0) as i64),
            Unit::Nanometer => Ok(*value as i64),
            Unit::Custom(symbol) => {
                if let Some(unit_def) = symbol_table.resolve_unit_symbol(symbol) {
                    let multiplier = unit_def.multiplier.unwrap_or(1.0);
                    Ok((value * multiplier * 1_000_000_000.0) as i64)
                } else {
                    Err(format!("Unknown unit symbol: '{}'", symbol))
                }
            }
            _ => Err(format!("Cannot convert {:?} to nanometers", unit)),
        },
        Expression::Variable { name, .. } => {
            if let Some(const_value) = symbol_table.get_all_constants().get(name) {
                Ok(*const_value as i64)
            } else {
                Err(format!("Unknown constant: '{}'", name))
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
            use hwc_parser::BinaryOperator;
            match operator {
                BinaryOperator::Add => Ok(left_nm + right_nm),
                BinaryOperator::Subtract => Ok(left_nm - right_nm),
                BinaryOperator::Multiply => Ok(left_nm * right_nm),
                BinaryOperator::Divide => {
                    if right_nm == 0 {
                        Err("Division by zero".into())
                    } else {
                        Ok(left_nm / right_nm)
                    }
                }
                BinaryOperator::Modulo => Ok(left_nm % right_nm),
            }
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_nm = evaluate_expression_to_nm(operand, symbol_table)?;
            use hwc_parser::UnaryOperator;
            match operator {
                UnaryOperator::Negate => Ok(-operand_nm),
                UnaryOperator::Plus => Ok(operand_nm),
            }
        }
        Expression::Grouped { expression, .. } => {
            evaluate_expression_to_nm(expression, symbol_table)
        }
        Expression::Percentage { .. } => {
            Err("Percentages cannot be evaluated without reference dimension".into())
        }
        Expression::AnchorReference { .. } => {
            Err("Anchor references cannot be evaluated without bounding box tracker.".into())
        }
    }
}

/// Evaluate an expression to milliamps (mA).
///
/// Handles current units: Ampere (×1000), Milliampere (as-is), Microampere (÷1000).
/// Dimensionless literals are treated as mA.
pub fn evaluate_expression_to_ma(
    expr: &Expression,
    symbol_table: &crate::SymbolTable,
) -> Result<f64, String> {
    match expr {
        Expression::Literal { value, .. } => Ok(*value as f64),
        Expression::FloatLiteral { value, .. } => Ok(*value),
        Expression::Measurement { value, unit, .. } => match unit {
            Unit::Ampere => Ok(value * 1_000.0),
            Unit::Milliampere => Ok(*value),
            Unit::Microampere => Ok(value / 1_000.0),
            _ => Err(format!("Cannot convert {:?} to milliamps", unit)),
        },
        Expression::Variable { name, .. } => {
            if let Some(const_value) = symbol_table.get_all_constants().get(name) {
                Ok(*const_value)
            } else {
                Err(format!("Unknown constant: '{}'", name))
            }
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let left_ma = evaluate_expression_to_ma(left, symbol_table)?;
            let right_ma = evaluate_expression_to_ma(right, symbol_table)?;
            use hwc_parser::BinaryOperator;
            match operator {
                BinaryOperator::Add => Ok(left_ma + right_ma),
                BinaryOperator::Subtract => Ok(left_ma - right_ma),
                BinaryOperator::Multiply => Ok(left_ma * right_ma),
                BinaryOperator::Divide => {
                    if right_ma == 0.0 {
                        Err("Division by zero".into())
                    } else {
                        Ok(left_ma / right_ma)
                    }
                }
                BinaryOperator::Modulo => Ok(left_ma % right_ma),
            }
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand_ma = evaluate_expression_to_ma(operand, symbol_table)?;
            use hwc_parser::UnaryOperator;
            match operator {
                UnaryOperator::Negate => Ok(-operand_ma),
                UnaryOperator::Plus => Ok(operand_ma),
            }
        }
        Expression::Grouped { expression, .. } => {
            evaluate_expression_to_ma(expression, symbol_table)
        }
        Expression::Percentage { .. } => {
            Err("Percentages cannot be evaluated as current".into())
        }
        Expression::AnchorReference { .. } => {
            Err("Anchor references cannot be evaluated as current".into())
        }
    }
}

pub fn measurement_to_nm(measurement: &Measurement, symbol_table: &crate::SymbolTable) -> i64 {
    let expr = Expression::Measurement {
        value: measurement.value,
        unit: measurement.unit.clone(),
        span: hwc_parser::Span::new(0, 0),
    };
    evaluate_expression_to_nm(&expr, symbol_table).unwrap()
}

pub(crate) fn z_expr_is_physical(z_expr: &Expression) -> bool {
    match z_expr {
        Expression::Measurement { .. } => true,
        Expression::Binary { left, right, .. } => {
            z_expr_is_physical(left) || z_expr_is_physical(right)
        }
        Expression::Unary { operand, .. } => z_expr_is_physical(operand),
        Expression::Grouped { expression, .. } => z_expr_is_physical(expression),
        Expression::Literal { .. } | Expression::FloatLiteral { .. } => false,
        Expression::Variable { .. } => true,
        Expression::Percentage { .. } => false,
        Expression::AnchorReference { .. } => true, // Anchor references resolve to physical coordinates
    }
}

pub fn apply_z_origin_physical(z_nm: i64, origin_z: hwc_parser::OriginZ, depth_nm: i64) -> i64 {
    match origin_z {
        hwc_parser::OriginZ::Bottom => z_nm,
        hwc_parser::OriginZ::Top => depth_nm.saturating_sub(z_nm),
    }
}

pub(crate) const DIMENSIONLESS_Z_ERROR: &str =
    "Z coordinates require physical units (e.g. z: 1.5mm). Dimensionless values like z: 1 are not supported.";

thread_local! {
    static LAST_Z_LOG: std::cell::RefCell<Option<(String, usize)>> = const { std::cell::RefCell::new(None) };
}

fn log_resolve_z(_msg: String) {
    /*
    LAST_Z_LOG.with(|last| {
        let mut last = last.borrow_mut();
        if let Some((prev_msg, count)) = last.as_mut() {
            if prev_msg == &msg {
                *count += 1;
                return;
            } else if *count > 1 {
                eprintln!("  (repeated {} times)", count);
            }
        }
        eprintln!("{}", msg);
        *last = Some((msg, 1));
    });
    */
}

pub fn resolve_coordinate_z_nm(
    z_expr: &Expression,
    ctx: &CoordinateContext,
    has_anchor_refs: bool,
) -> Result<i64, String> {
    if has_anchor_refs && z_expr.contains_anchor_reference() {
        let tracker = ctx.bbox_tracker.ok_or("BoundingBoxTracker required")?;
        let result = super::placement::coordinate_evaluation::evaluate_coordinate_with_anchors(
            z_expr,
            ctx.symbol_table,
            tracker,
            super::placement::coordinate_evaluation::CoordinateAxis::Z,
            ctx.origin.z,
        )
        .map_err(|e| e.to_string());

        if let Ok(val) = result {
            log_resolve_z(format!("[Z-Axis] Anchor: {:?} -> {}nm", z_expr, val));
        }
        return result;
    }

    if !z_expr_is_physical(z_expr) {
        return Err(DIMENSIONLESS_Z_ERROR.to_string());
    }

    let z_nm = evaluate_expression_to_nm(z_expr, ctx.symbol_table)?;
    let final_z = apply_z_origin_physical(z_nm, ctx.origin.z, ctx.space_dimensions.depth_nm);

    let expr_summary = match z_expr {
        Expression::Measurement { value, unit, .. } => format!("{:.3}{:?}", value, unit),
        _ => format!("{:?}", z_expr),
    };
    log_resolve_z(format!("[Z-Axis] {} -> {}nm", expr_summary, final_z));

    Ok(final_z)
}

pub fn coordinate_to_point(coord: &Coordinate, ctx: &CoordinateContext) -> Point3D {
    let (x_expr, y_expr, z_expr) = match coord {
        Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
            (x, y, z)
        }
        Coordinate::Relative(_) => panic!("Relative coordinates must be resolved"),
    };

    let has_anchor_refs = x_expr.contains_anchor_reference()
        || y_expr.contains_anchor_reference()
        || z_expr.contains_anchor_reference();

    let x_nm = if let Ok(Value::Percentage(pct)) = x_expr.evaluate(ctx.eval_context) {
        ((pct / 100.0) * ctx.space_dimensions.width_nm as f64) as i64
    } else if has_anchor_refs && x_expr.contains_anchor_reference() {
        let tracker = ctx.bbox_tracker.expect("Tracker required");
        super::placement::coordinate_evaluation::evaluate_coordinate_with_anchors(
            x_expr,
            ctx.symbol_table,
            tracker,
            CoordinateAxis::X,
            ctx.origin.z,
        )
        .unwrap()
    } else {
        evaluate_expression_to_nm(x_expr, ctx.symbol_table).unwrap()
    };

    let y_nm = if let Ok(Value::Percentage(pct)) = y_expr.evaluate(ctx.eval_context) {
        ((pct / 100.0) * ctx.space_dimensions.height_nm as f64) as i64
    } else if has_anchor_refs && y_expr.contains_anchor_reference() {
        let tracker = ctx.bbox_tracker.expect("Tracker required");
        super::placement::coordinate_evaluation::evaluate_coordinate_with_anchors(
            y_expr,
            ctx.symbol_table,
            tracker,
            CoordinateAxis::Y,
            ctx.origin.z,
        )
        .unwrap()
    } else {
        evaluate_expression_to_nm(y_expr, ctx.symbol_table).unwrap()
    };

    let z_nm = resolve_coordinate_z_nm(z_expr, ctx, has_anchor_refs).unwrap();

    use hwc_parser::OriginXY;
    let final_x_nm = match ctx.origin.xy {
        OriginXY::TL | OriginXY::BL => x_nm,
        OriginXY::TR | OriginXY::BR => ctx.space_dimensions.width_nm - x_nm,
    };
    let final_y_nm = match ctx.origin.xy {
        OriginXY::BL | OriginXY::BR => y_nm,
        OriginXY::TL | OriginXY::TR => ctx.space_dimensions.height_nm - y_nm,
    };

    Point3D::new(final_x_nm, final_y_nm, z_nm)
}

pub fn spanning_coordinate_to_point(
    coord: &Coordinate,
    ctx: &CoordinateContext,
    _is_end: bool,
) -> Result<Point3D, String> {
    let (x_expr, y_expr, z_expr) = match coord {
        Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
            (x, y, z)
        }
        Coordinate::Relative(_) => panic!("Relative coordinates must be resolved"),
    };

    let has_anchor_refs = x_expr.contains_anchor_reference()
        || y_expr.contains_anchor_reference()
        || z_expr.contains_anchor_reference();

    let x_nm = evaluate_expression_to_nm(x_expr, ctx.symbol_table)?;
    let y_nm = evaluate_expression_to_nm(y_expr, ctx.symbol_table)?;
    let z_nm = resolve_coordinate_z_nm(z_expr, ctx, has_anchor_refs)?;

    use hwc_parser::OriginXY;
    let final_x_nm = match ctx.origin.xy {
        OriginXY::TL | OriginXY::BL => x_nm,
        OriginXY::TR | OriginXY::BR => ctx.space_dimensions.width_nm - x_nm,
    };
    let final_y_nm = match ctx.origin.xy {
        OriginXY::BL | OriginXY::BR => y_nm,
        OriginXY::TL | OriginXY::TR => ctx.space_dimensions.height_nm - y_nm,
    };

    Ok(Point3D::new(final_x_nm, final_y_nm, z_nm))
}
