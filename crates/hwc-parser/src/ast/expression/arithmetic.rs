//! Typed arithmetic over evaluated [`Value`]s.
//!
//! ## Physics-Correct Math
//!
//! Operations are dispatched on the *dimension* of both operands, not just
//! their numeric payload. Measurements only combine with other measurements
//! (after unit normalization) or with dimensionless scalars used as
//! multipliers/divisors. Everything else is rejected with a descriptive error
//! so unit mistakes surface at compile time instead of silently producing
//! wrong geometry.

use super::operators::{BinaryOperator, UnaryOperator};
use super::value::Value;
use crate::ast::Unit;

/// Apply a binary operator to two already-evaluated values.
pub(super) fn evaluate_binary_values(
    left_val: &Value,
    operator: &BinaryOperator,
    right_val: &Value,
) -> Result<Value, String> {
    match (left_val, right_val) {
        (Value::Number(l), Value::Number(r)) => {
            // Both integers: normal integer arithmetic
            operator.apply(*l, *r).map(Value::Number)
        }
        (Value::Float(l), Value::Float(r)) => {
            // Both floats: normal float arithmetic
            apply_op_f64(*l, *r, operator).map(Value::Float)
        }
        (Value::Number(l), Value::Float(r)) => {
            // Integer op Float: promote to float
            apply_op_f64(*l as f64, *r, operator).map(Value::Float)
        }
        (Value::Float(l), Value::Number(r)) => {
            // Float op Integer: promote to float
            apply_op_f64(*l, *r as f64, operator).map(Value::Float)
        }
        (
            Value::Measurement { value: lv, unit: lu },
            Value::Measurement { value: rv, unit: ru },
        ) => measurement_op_measurement(*lv, lu, operator, *rv, ru),
        (Value::Measurement { value: lv, unit: lu }, Value::Number(r)) => {
            // ARCHITECTURAL ERROR: Mixing physical measurements with bare scalars
            // This is mathematically invalid unless the scalar represents a multiplier
            match operator {
                BinaryOperator::Multiply | BinaryOperator::Divide => {
                    // Scaling operations are valid: 50µm * 2 = 100µm
                    let result = apply_op_f64(*lv, *r as f64, operator)?;
                    Ok(Value::Measurement {
                        value: result,
                        unit: lu.clone(),
                    })
                }
                _ => Err(format!(
                    "Cannot perform {:?} between measurement ({:?}) and dimensionless number ({}). \
                     This operation is mathematically invalid. Did you mean to use a measurement unit?",
                    operator, lu, r
                )),
            }
        }
        (Value::Number(l), Value::Measurement { value: rv, unit: ru }) => {
            // ARCHITECTURAL ERROR: Same as above, but reversed operands
            match operator {
                BinaryOperator::Multiply => {
                    // Scaling is valid: 2 * 50µm = 100µm
                    let result = apply_op_f64(*l as f64, *rv, operator)?;
                    Ok(Value::Measurement {
                        value: result,
                        unit: ru.clone(),
                    })
                }
                _ => Err(format!(
                    "Cannot perform {:?} between dimensionless number ({}) and measurement ({:?}). \
                     This operation is mathematically invalid. Did you mean to use a measurement unit?",
                    operator, l, ru
                )),
            }
        }
        (Value::Measurement { value: lv, unit: lu }, Value::Float(r)) => {
            // Measurement op Float: treat Float as a multiplier/divisor
            match operator {
                BinaryOperator::Multiply | BinaryOperator::Divide => {
                    let result = apply_op_f64(*lv, *r, operator)?;
                    Ok(Value::Measurement {
                        value: result,
                        unit: lu.clone(),
                    })
                }
                _ => Err(format!(
                    "Cannot perform {:?} between measurement ({:?}) and dimensionless float ({}). \
                     This operation is mathematically invalid. Did you mean to use a measurement unit?",
                    operator, lu, r
                )),
            }
        }
        (Value::Float(l), Value::Measurement { value: rv, unit: ru }) => {
            // Float op Measurement: treat Float as a multiplier
            match operator {
                BinaryOperator::Multiply => {
                    let result = apply_op_f64(*l, *rv, operator)?;
                    Ok(Value::Measurement {
                        value: result,
                        unit: ru.clone(),
                    })
                }
                _ => Err(format!(
                    "Cannot perform {:?} between dimensionless float ({}) and measurement ({:?}). \
                     This operation is mathematically invalid. Did you mean to use a measurement unit?",
                    operator, l, ru
                )),
            }
        }
        (Value::Percentage(l), Value::Number(r)) => {
            // Percentage op Number: apply to percentage value
            let result = apply_op_f64(*l, *r as f64, operator)?;
            Ok(Value::Percentage(result))
        }
        (Value::Percentage(l), Value::Float(r)) => {
            // Percentage op Float: apply to percentage value
            let result = apply_op_f64(*l, *r, operator)?;
            Ok(Value::Percentage(result))
        }
        (Value::Number(l), Value::Percentage(r)) => {
            // Number op Percentage: apply to percentage value
            let result = apply_op_f64(*l as f64, *r, operator)?;
            Ok(Value::Percentage(result))
        }
        (Value::Float(l), Value::Percentage(r)) => {
            // Float op Percentage: apply to percentage value
            let result = apply_op_f64(*l, *r, operator)?;
            Ok(Value::Percentage(result))
        }
        (Value::Percentage(l), Value::Percentage(r)) => {
            // Percentage op Percentage
            let result = apply_op_f64(*l, *r, operator)?;
            Ok(Value::Percentage(result))
        }
        // Mixed percentage and measurement operations
        (Value::Percentage(_), Value::Measurement { .. })
        | (Value::Measurement { .. }, Value::Percentage(_)) => {
            Err("Cannot perform arithmetic between percentages and measurements directly. Percentages must be resolved to physical units first.".into())
        }
    }
}

/// CLEAN ARCHITECTURE: Physics-Correct Math (Unit Normalization)
/// Both operands are measurements with units preserved.
fn measurement_op_measurement(
    lv: f64,
    lu: &Unit,
    operator: &BinaryOperator,
    rv: f64,
    ru: &Unit,
) -> Result<Value, String> {
    // For comparison operators, convert to same units and return boolean (0 or 1)
    if operator.is_comparison() {
        // Normalize both to nanometers for comparison
        let l_nm = Value::Measurement {
            value: lv,
            unit: lu.clone(),
        }
        .to_nanometers()?;
        let r_nm = Value::Measurement {
            value: rv,
            unit: ru.clone(),
        }
        .to_nanometers()?;
        let result = operator.apply(l_nm, r_nm)?;
        return Ok(Value::Number(result)); // Return boolean as Number (0 or 1)
    }

    // For arithmetic operations
    if lu == ru {
        // Same units: safe to perform arithmetic
        let result = apply_op_f64(lv, rv, operator)?;
        Ok(Value::Measurement {
            value: result,
            unit: lu.clone(),
        })
    } else {
        // Different units: normalize both to nanometers
        let l_nm = Value::Measurement {
            value: lv,
            unit: lu.clone(),
        }
        .to_nanometers()?;
        let r_nm = Value::Measurement {
            value: rv,
            unit: ru.clone(),
        }
        .to_nanometers()?;
        let result_nm = operator.apply(l_nm, r_nm)?;
        // Return result in nanometers (normalized unit)
        Ok(Value::Measurement {
            value: result_nm as f64,
            unit: Unit::Nanometer,
        })
    }
}

/// Apply a unary operator to an already-evaluated value.
pub(super) fn evaluate_unary_value(
    operator: &UnaryOperator,
    operand_val: Value,
) -> Result<Value, String> {
    match operand_val {
        Value::Number(n) => operator.apply(n).map(Value::Number),
        Value::Float(f) => {
            let result = match operator {
                UnaryOperator::Negate => -f,
                UnaryOperator::Plus => f,
                UnaryOperator::Not => {
                    if f == 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            Ok(Value::Float(result))
        }
        Value::Measurement { value, unit } => match operator {
            UnaryOperator::Negate => Ok(Value::Measurement {
                value: -value,
                unit,
            }),
            UnaryOperator::Plus => Ok(Value::Measurement { value, unit }),
            UnaryOperator::Not => Err(
                "Logical NOT cannot be applied to measurements. Use comparison operators instead."
                    .into(),
            ),
        },
        Value::Percentage(pct) => match operator {
            UnaryOperator::Negate => Ok(Value::Percentage(-pct)),
            UnaryOperator::Plus => Ok(Value::Percentage(pct)),
            UnaryOperator::Not => Err(
                "Logical NOT cannot be applied to percentages. Use comparison operators instead."
                    .into(),
            ),
        },
    }
}

/// Helper function to apply binary operators to f64 values
pub(super) fn apply_op_f64(
    left: f64,
    right: f64,
    operator: &BinaryOperator,
) -> Result<f64, String> {
    match operator {
        BinaryOperator::Add => Ok(left + right),
        BinaryOperator::Subtract => Ok(left - right),
        BinaryOperator::Multiply => Ok(left * right),
        BinaryOperator::Divide => {
            if right == 0.0 {
                Err("Division by zero".into())
            } else {
                Ok(left / right)
            }
        }
        BinaryOperator::Modulo => Err("Modulo not supported for floating point values".into()),
        // Comparison operators return 1.0 for true, 0.0 for false
        BinaryOperator::Equal => Ok(if (left - right).abs() < f64::EPSILON {
            1.0
        } else {
            0.0
        }),
        BinaryOperator::NotEqual => Ok(if (left - right).abs() >= f64::EPSILON {
            1.0
        } else {
            0.0
        }),
        BinaryOperator::LessThan => Ok(if left < right { 1.0 } else { 0.0 }),
        BinaryOperator::GreaterThan => Ok(if left > right { 1.0 } else { 0.0 }),
        BinaryOperator::LessThanOrEqual => Ok(if left <= right { 1.0 } else { 0.0 }),
        BinaryOperator::GreaterThanOrEqual => Ok(if left >= right { 1.0 } else { 0.0 }),
        // Boolean operators (treat non-zero as true, zero as false)
        BinaryOperator::And => Ok(if left != 0.0 && right != 0.0 {
            1.0
        } else {
            0.0
        }),
        BinaryOperator::Or => Ok(if left != 0.0 || right != 0.0 {
            1.0
        } else {
            0.0
        }),
    }
}
