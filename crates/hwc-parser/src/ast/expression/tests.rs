//! Tests for expression evaluation.
//!
//! These cover the behavior that was previously verified inline in the
//! monolithic `ast/expression.rs`, plus regression coverage for the helpers
//! extracted into `arithmetic.rs`, `functions.rs` and `display.rs`.

use super::*;
use crate::ast::{Span, Unit};
use rustc_hash::FxHashMap;

/// Build a zero-width span for terse test fixtures.
fn sp() -> Span {
    Span::new(0, 0)
}

/// Build an integer literal expression.
fn lit(value: i64) -> Expression {
    Expression::Literal { value, span: sp() }
}

/// Build a measurement expression.
fn meas(value: f64, unit: Unit) -> Expression {
    Expression::Measurement {
        value,
        unit,
        span: sp(),
    }
}

/// Build a binary expression.
fn bin(left: Expression, operator: BinaryOperator, right: Expression) -> Expression {
    Expression::Binary {
        left: Box::new(left),
        operator,
        right: Box::new(right),
        span: sp(),
    }
}

/// Build a function-call expression.
fn call(name: &str, arguments: Vec<Expression>) -> Expression {
    Expression::FunctionCall {
        name: name.into(),
        arguments,
        span: sp(),
    }
}

#[test]
fn test_evaluate_literal() {
    let expr = lit(42);
    assert_eq!(expr.evaluate_const().unwrap(), Value::Number(42));
}

#[test]
fn test_evaluate_addition() {
    let expr = bin(lit(10), BinaryOperator::Add, lit(5));
    assert_eq!(expr.evaluate_const().unwrap(), Value::Number(15));
}

#[test]
fn test_evaluate_with_variable() {
    let expr = bin(
        lit(20),
        BinaryOperator::Add,
        bin(
            Expression::Variable {
                name: "i".into(),
                span: sp(),
            },
            BinaryOperator::Multiply,
            lit(2),
        ),
    );

    let mut context = FxHashMap::default();
    context.insert("i".into(), Value::Number(5));
    assert_eq!(expr.evaluate(&context).unwrap(), Value::Number(30)); // 20 + (5 * 2) = 30
}

#[test]
fn test_evaluate_undefined_variable() {
    let expr = Expression::Variable {
        name: "x".into(),
        span: sp(),
    };
    assert!(expr.evaluate_const().is_err());
}

#[test]
fn test_evaluate_division_by_zero() {
    let expr = bin(lit(10), BinaryOperator::Divide, lit(0));
    assert!(expr.evaluate_const().is_err());
}

#[test]
fn test_same_unit_arithmetic_preserves_unit() {
    let expr = bin(
        meas(2.0, Unit::Millimeter),
        BinaryOperator::Add,
        meas(3.0, Unit::Millimeter),
    );
    assert_eq!(
        expr.evaluate_const().unwrap(),
        Value::Measurement {
            value: 5.0,
            unit: Unit::Millimeter
        }
    );
}

#[test]
fn test_mixed_unit_arithmetic_normalizes_to_nanometers() {
    // 1mm + 500µm => 1_000_000nm + 500_000nm
    let expr = bin(
        meas(1.0, Unit::Millimeter),
        BinaryOperator::Add,
        meas(500.0, Unit::Micrometer),
    );
    assert_eq!(
        expr.evaluate_const().unwrap(),
        Value::Measurement {
            value: 1_500_000.0,
            unit: Unit::Nanometer
        }
    );
}

#[test]
fn test_measurement_comparison_returns_boolean_number() {
    let expr = bin(
        meas(1.0, Unit::Millimeter),
        BinaryOperator::GreaterThan,
        meas(500.0, Unit::Micrometer),
    );
    assert_eq!(expr.evaluate_const().unwrap(), Value::Number(1));
}

#[test]
fn test_measurement_scaling_by_scalar_is_allowed() {
    let expr = bin(
        meas(50.0, Unit::Micrometer),
        BinaryOperator::Multiply,
        lit(2),
    );
    assert_eq!(
        expr.evaluate_const().unwrap(),
        Value::Measurement {
            value: 100.0,
            unit: Unit::Micrometer
        }
    );
}

#[test]
fn test_measurement_plus_scalar_is_rejected() {
    let expr = bin(meas(50.0, Unit::Micrometer), BinaryOperator::Add, lit(2));
    assert!(expr.evaluate_const().is_err());
}

#[test]
fn test_percentage_and_measurement_mix_is_rejected() {
    let expr = bin(
        Expression::Percentage {
            value: 50.0,
            span: sp(),
        },
        BinaryOperator::Add,
        meas(1.0, Unit::Millimeter),
    );
    assert!(expr.evaluate_const().is_err());
}

#[test]
fn test_unary_negate_measurement() {
    let expr = Expression::Unary {
        operator: UnaryOperator::Negate,
        operand: Box::new(meas(3.0, Unit::Nanometer)),
        span: sp(),
    };
    assert_eq!(
        expr.evaluate_const().unwrap(),
        Value::Measurement {
            value: -3.0,
            unit: Unit::Nanometer
        }
    );
}

#[test]
fn test_builtin_sqrt_and_abs() {
    assert_eq!(
        call("sqrt", vec![lit(4)]).evaluate_const().unwrap(),
        Value::Float(2.0)
    );
    // abs() is dimension-preserving
    assert_eq!(
        call("abs", vec![meas(-2.5, Unit::Millimeter)])
            .evaluate_const()
            .unwrap(),
        Value::Measurement {
            value: 2.5,
            unit: Unit::Millimeter
        }
    );
}

#[test]
fn test_builtin_min_max_variadic() {
    let args = vec![lit(3), lit(1), lit(2)];
    assert_eq!(
        call("min", args.clone()).evaluate_const().unwrap(),
        Value::Float(1.0)
    );
    assert_eq!(
        call("max", args).evaluate_const().unwrap(),
        Value::Float(3.0)
    );
}

#[test]
fn test_builtin_arity_and_domain_errors() {
    let wrong_arity = call("sin", vec![]).evaluate_const().unwrap_err();
    assert_eq!(wrong_arity, "sin() expects 1 argument, got 0");

    let domain = call("sqrt", vec![lit(-1)]).evaluate_const().unwrap_err();
    assert!(domain.contains("must be non-negative"));

    let unknown = call("frobnicate", vec![lit(1)])
        .evaluate_const()
        .unwrap_err();
    assert!(unknown.contains("Available functions"));
}

#[test]
fn test_operator_precedence_ordering() {
    assert!(BinaryOperator::Or.precedence() < BinaryOperator::And.precedence());
    assert!(BinaryOperator::And.precedence() < BinaryOperator::Equal.precedence());
    assert!(BinaryOperator::Add.precedence() < BinaryOperator::Multiply.precedence());
}

#[test]
fn test_display_round_trip() {
    let expr = bin(lit(10), BinaryOperator::Add, lit(5));
    assert_eq!(expr.to_string(), "10 + 5");

    let grouped = Expression::Grouped {
        expression: Box::new(expr),
        span: sp(),
    };
    assert_eq!(grouped.to_string(), "(10 + 5)");

    assert_eq!(call("sin", vec![lit(1)]).to_string(), "sin(1)");
}
