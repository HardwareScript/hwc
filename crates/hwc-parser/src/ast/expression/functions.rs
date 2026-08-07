//! Built-in compile-time functions (`sin`, `sqrt`, `min`, `radians`, ...).
//!
//! All built-ins are pure and evaluated at compile time. Trigonometric
//! functions operate on radians; use `radians()`/`degrees()` to convert.

use std::f64::consts::PI;

use super::types::Expression;
use super::value::{EvaluationContext, Value};
use crate::ast::Span;

/// Comma-separated list of every supported built-in, used in the "unknown
/// function" diagnostic so users get an actionable suggestion.
const AVAILABLE_FUNCTIONS: &str = "sin, cos, tan, asin, acos, atan, atan2, sqrt, abs, pow, exp, ln, log10, log2, floor, ceil, round, min, max, radians, degrees";

/// Evaluate a function call expression
pub(super) fn evaluate_function_call(
    name: &str,
    arguments: &[Expression],
    context: &EvaluationContext,
    _span: Span,
) -> Result<Value, String> {
    match name {
        // Trigonometric functions (expect radians)
        "sin" => Ok(Value::Float(arg1(name, arguments, context)?.sin())),
        "cos" => Ok(Value::Float(arg1(name, arguments, context)?.cos())),
        "tan" => Ok(Value::Float(arg1(name, arguments, context)?.tan())),
        "asin" => {
            let arg = arg1(name, arguments, context)?;
            require_unit_range(name, arg)?;
            Ok(Value::Float(arg.asin()))
        }
        "acos" => {
            let arg = arg1(name, arguments, context)?;
            require_unit_range(name, arg)?;
            Ok(Value::Float(arg.acos()))
        }
        "atan" => Ok(Value::Float(arg1(name, arguments, context)?.atan())),
        "atan2" => {
            let (y, x) = arg2(name, "y, x", arguments, context)?;
            Ok(Value::Float(y.atan2(x)))
        }

        // Mathematical functions
        "sqrt" => {
            let arg = arg1(name, arguments, context)?;
            if arg < 0.0 {
                return Err(format!("sqrt() argument must be non-negative, got {}", arg));
            }
            Ok(Value::Float(arg.sqrt()))
        }
        "abs" => {
            if arguments.len() != 1 {
                return Err(format!("abs() expects 1 argument, got {}", arguments.len()));
            }
            // `abs` is dimension-preserving: it keeps units/percentages intact.
            match arguments[0].evaluate(context)? {
                Value::Number(n) => Ok(Value::Number(n.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                Value::Measurement { value, unit } => Ok(Value::Measurement {
                    value: value.abs(),
                    unit,
                }),
                Value::Percentage(p) => Ok(Value::Percentage(p.abs())),
            }
        }
        "pow" => {
            let (base, exp) = arg2(name, "base, exponent", arguments, context)?;
            Ok(Value::Float(base.powf(exp)))
        }
        "exp" => Ok(Value::Float(arg1(name, arguments, context)?.exp())),
        "ln" => {
            let arg = arg1(name, arguments, context)?;
            require_positive(name, arg)?;
            Ok(Value::Float(arg.ln()))
        }
        "log" | "log10" => {
            // Both spellings report as `log10()` for consistency.
            let arg = arg1("log10", arguments, context)?;
            require_positive("log10", arg)?;
            Ok(Value::Float(arg.log10()))
        }
        "log2" => {
            let arg = arg1(name, arguments, context)?;
            require_positive(name, arg)?;
            Ok(Value::Float(arg.log2()))
        }

        // Rounding functions
        "floor" => Ok(Value::Float(arg1(name, arguments, context)?.floor())),
        "ceil" => Ok(Value::Float(arg1(name, arguments, context)?.ceil())),
        "round" => Ok(Value::Float(arg1(name, arguments, context)?.round())),

        // Utility functions
        "min" => Ok(Value::Float(fold_variadic(
            name,
            arguments,
            context,
            |acc, val| val < acc,
        )?)),
        "max" => Ok(Value::Float(fold_variadic(
            name,
            arguments,
            context,
            |acc, val| val > acc,
        )?)),

        // Unit conversion helper (degrees to radians)
        "radians" | "rad" => {
            let degrees = arg1_hint("radians", "degrees", arguments, context)?;
            Ok(Value::Float(degrees * PI / 180.0))
        }
        "degrees" | "deg" => {
            let radians = arg1_hint("degrees", "radians", arguments, context)?;
            Ok(Value::Float(radians * 180.0 / PI))
        }

        _ => Err(format!(
            "Unknown function '{}'. Available functions: {}",
            name, AVAILABLE_FUNCTIONS
        )),
    }
}

/// Evaluate the single expected argument of a unary built-in as a scalar.
fn arg1(label: &str, arguments: &[Expression], context: &EvaluationContext) -> Result<f64, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "{}() expects 1 argument, got {}",
            label,
            arguments.len()
        ));
    }
    arguments[0].evaluate(context)?.as_number()
}

/// Same as [`arg1`], but names the argument in the arity error (e.g. "degrees").
fn arg1_hint(
    label: &str,
    hint: &str,
    arguments: &[Expression],
    context: &EvaluationContext,
) -> Result<f64, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "{}() expects 1 argument ({}), got {}",
            label,
            hint,
            arguments.len()
        ));
    }
    arguments[0].evaluate(context)?.as_number()
}

/// Evaluate the two expected arguments of a binary built-in as scalars.
fn arg2(
    label: &str,
    hint: &str,
    arguments: &[Expression],
    context: &EvaluationContext,
) -> Result<(f64, f64), String> {
    if arguments.len() != 2 {
        return Err(format!(
            "{}() expects 2 arguments ({}), got {}",
            label,
            hint,
            arguments.len()
        ));
    }
    let first = arguments[0].evaluate(context)?.as_number()?;
    let second = arguments[1].evaluate(context)?.as_number()?;
    Ok((first, second))
}

/// Fold a variadic built-in (`min`/`max`), replacing the accumulator whenever
/// `should_replace(accumulator, candidate)` is true.
fn fold_variadic(
    label: &str,
    arguments: &[Expression],
    context: &EvaluationContext,
    should_replace: fn(f64, f64) -> bool,
) -> Result<f64, String> {
    if arguments.len() < 2 {
        return Err(format!(
            "{}() expects at least 2 arguments, got {}",
            label,
            arguments.len()
        ));
    }
    let mut acc = arguments[0].evaluate(context)?.as_number()?;
    for arg in &arguments[1..] {
        let val = arg.evaluate(context)?.as_number()?;
        if should_replace(acc, val) {
            acc = val;
        }
    }
    Ok(acc)
}

/// Domain guard for `asin`/`acos`.
fn require_unit_range(label: &str, arg: f64) -> Result<(), String> {
    if !(-1.0..=1.0).contains(&arg) {
        return Err(format!(
            "{}() argument must be in range [-1, 1], got {}",
            label, arg
        ));
    }
    Ok(())
}

/// Domain guard for the logarithm family.
fn require_positive(label: &str, arg: f64) -> Result<(), String> {
    if arg <= 0.0 {
        return Err(format!(
            "{}() argument must be positive, got {}",
            label, arg
        ));
    }
    Ok(())
}
