//! Expression evaluation entry points.
//!
//! This module only performs AST traversal and variable lookup. The typed math
//! rules (dimensional analysis) live in `arithmetic.rs`, and built-in function
//! dispatch lives in `functions.rs`.

use rustc_hash::FxHashMap;

use super::arithmetic::{evaluate_binary_values, evaluate_unary_value};
use super::functions::evaluate_function_call;
use super::types::Expression;
use super::value::{EvaluationContext, Value};

impl Expression {
    /// Evaluate this expression to a concrete value (number, measurement, or percentage)
    /// Returns an error if the expression contains undefined variables or division by zero
    pub fn evaluate(&self, context: &EvaluationContext) -> Result<Value, String> {
        match self {
            Expression::Literal { value, .. } => Ok(Value::Number(*value)),
            Expression::FloatLiteral { value, .. } => Ok(Value::Float(*value)),
            Expression::Measurement { value, unit, .. } => Ok(Value::Measurement {
                value: *value,
                unit: unit.clone(),
            }),
            Expression::Percentage { value, .. } => Ok(Value::Percentage(*value)),
            Expression::Variable { name, .. } => {
                if name == "true" {
                    Ok(Value::Number(1))
                } else if name == "false" {
                    Ok(Value::Number(0))
                } else {
                    context
                        .get(name)
                        .cloned()
                        .ok_or_else(|| format!("Undefined variable '{}' in expression", name))
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left_val = left.evaluate(context)?;
                let right_val = right.evaluate(context)?;
                evaluate_binary_values(&left_val, operator, &right_val)
            }
            Expression::Unary {
                operator, operand, ..
            } => {
                let operand_val = operand.evaluate(context)?;
                evaluate_unary_value(operator, operand_val)
            }
            Expression::Grouped { expression, .. } => expression.evaluate(context),
            Expression::AnchorReference { .. } => {
                // Anchor references cannot be evaluated without the bounding box tracker
                // They must be resolved by the compiler's constraint solver
                Err("Anchor references require constraint solver context and cannot be evaluated in the parser. \
                     This expression should be evaluated by the compiler using evaluate_coordinate_with_anchors.".into())
            }
            Expression::Coordinate { .. } => {
                // Coordinate literals cannot be evaluated to a single value
                // They must be handled by the coordinate evaluation system
                Err(
                    "Coordinate literals cannot be evaluated to a single value. \
                     They must be resolved by the coordinate evaluation system."
                        .into(),
                )
            }
            Expression::FunctionCall {
                name,
                arguments,
                span,
            } => {
                // Evaluate function calls (sin, cos, tan, sqrt, etc.)
                evaluate_function_call(name, arguments, context, *span)
            }
        }
    }

    /// Evaluate with an empty context (no variables)
    pub fn evaluate_const(&self) -> Result<Value, String> {
        self.evaluate(&FxHashMap::default())
    }

    /// Try to evaluate as a constant (no variables)
    /// Returns None if the expression contains variables
    pub fn try_evaluate_const(&self) -> Option<Value> {
        self.evaluate_const().ok()
    }
}
