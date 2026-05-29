//! Expression AST building and evaluation utilities
//!
//! Provides proper mathematical evaluation of index expressions to avoid
//! the "Carry[11]" bug where string substitution treats "i+1" as "11" instead of 2.

use crate::ir::errors::IrError;
use hwc_parser::Expression;

/// Build a simple Expression AST from a string
///
/// **Purpose**: Convert string like "i+1" into a proper AST for mathematical evaluation
///
/// **Why This Matters**: The difference between "Carry[11]" and "Carry[2]"
/// - String substitution: "i+1" → "1+1" → "11" (string concatenation) ❌
/// - AST evaluation: i+1 → 1+1 → 2 (mathematical addition) ✅
///
/// **Supported Patterns**:
/// - Literals: `0`, `1`, `42`
/// - Variables: `i`, `j`, `k`
/// - Binary ops: `i+1`, `i-1`, `i*2`, `i/2`
/// - Nested: `(i+1)*2`, `i+(j-1)`
pub fn build_simple_expression_ast(expr_str: &str) -> Result<Expression, IrError> {
    let expr_str = expr_str.trim();

    // Try to parse as a literal number first
    if let Ok(literal) = expr_str.parse::<i64>() {
        return Ok(Expression::Literal {
            value: literal,
            span: hwc_parser::lexer::Span::new(0, expr_str.len()),
        });
    }

    // Check if it's a simple variable (single identifier)
    if expr_str.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Ok(Expression::Variable {
            name: expr_str.into(),
            span: hwc_parser::lexer::Span::new(0, expr_str.len()),
        });
    }

    // Try to find binary operators (in order of precedence: +/- last, then *//)
    // Search from right to left to handle left-associativity correctly
    for op_char in ['+', '-'] {
        if let Some(pos) = expr_str.rfind(op_char) {
            // Skip if this is a negative sign at the start
            if pos == 0 && op_char == '-' {
                continue;
            }

            let left_str = expr_str[..pos].trim();
            let right_str = expr_str[pos + 1..].trim();

            let left = build_simple_expression_ast(left_str)?;
            let right = build_simple_expression_ast(right_str)?;

            let operator = match op_char {
                '+' => hwc_parser::ast::BinaryOperator::Add,
                '-' => hwc_parser::ast::BinaryOperator::Subtract,
                _ => unreachable!(),
            };

            return Ok(Expression::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                span: hwc_parser::lexer::Span::new(0, expr_str.len()),
            });
        }
    }

    // Then try multiplication and division
    for op_char in ['*', '/'] {
        if let Some(pos) = expr_str.rfind(op_char) {
            let left_str = expr_str[..pos].trim();
            let right_str = expr_str[pos + 1..].trim();

            let left = build_simple_expression_ast(left_str)?;
            let right = build_simple_expression_ast(right_str)?;

            let operator = match op_char {
                '*' => hwc_parser::ast::BinaryOperator::Multiply,
                '/' => hwc_parser::ast::BinaryOperator::Divide,
                _ => unreachable!(),
            };

            return Ok(Expression::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                span: hwc_parser::lexer::Span::new(0, expr_str.len()),
            });
        }
    }

    // Handle parentheses
    if expr_str.starts_with('(') && expr_str.ends_with(')') {
        let inner = &expr_str[1..expr_str.len() - 1];
        let inner_expr = build_simple_expression_ast(inner)?;
        return Ok(Expression::Grouped {
            expression: Box::new(inner_expr),
            span: hwc_parser::lexer::Span::new(0, expr_str.len()),
        });
    }

    Err(IrError::InvalidExpression(format!(
        "Cannot parse expression: '{}'",
        expr_str
    )))
}

/// Simple arithmetic evaluator for anchor index expressions
///
/// Supports: +, -, *, / with integer operands
/// Returns i64 to allow detection of negative results
pub fn evaluate_simple_arithmetic(expr: &str) -> Result<i64, String> {
    let expr = expr.trim();

    // Try direct parse first
    if let Ok(n) = expr.parse::<i64>() {
        return Ok(n);
    }

    // Try to find operators (in order of precedence)
    // Division and multiplication first
    for op in ['/', '*'] {
        if let Some(pos) = expr.rfind(op) {
            let left = expr[..pos].trim();
            let right = expr[pos + 1..].trim();

            let left_val = evaluate_simple_arithmetic(left)?;
            let right_val = evaluate_simple_arithmetic(right)?;

            return match op {
                '*' => Ok(left_val * right_val),
                '/' => {
                    if right_val == 0 {
                        Err("Division by zero".to_string())
                    } else {
                        Ok(left_val / right_val)
                    }
                }
                _ => unreachable!(),
            };
        }
    }

    // Then addition and subtraction
    for op in ['+', '-'] {
        if let Some(pos) = expr.rfind(op) {
            // Skip if this is a negative sign at the start
            if pos == 0 && op == '-' {
                continue;
            }

            let left = expr[..pos].trim();
            let right = expr[pos + 1..].trim();

            let left_val = evaluate_simple_arithmetic(left)?;
            let right_val = evaluate_simple_arithmetic(right)?;

            return match op {
                '+' => Ok(left_val + right_val),
                '-' => Ok(left_val - right_val),
                _ => unreachable!(),
            };
        }
    }

    Err(format!("Cannot parse expression: {}", expr))
}

/// Evaluate an index expression in an anchor name
///
/// Supports common patterns:
/// - `i` → current value
/// - `i-1` → value - 1
/// - `i+1` → value + 1
/// - `i*2` → value * 2
/// - `i/2` → value / 2
/// - Literal numbers: `0`, `1`, etc.
///
/// **Safety Guards**:
/// - Negative results cause error (can't reference Adder[-1])
/// - Division by zero causes error
pub fn evaluate_anchor_index_expression(
    expr_str: &str,
    variable: &str,
    value: usize,
) -> Result<usize, IrError> {
    let expr_str = expr_str.trim();

    // Try to parse as a literal number first
    if let Ok(literal) = expr_str.parse::<usize>() {
        return Ok(literal);
    }

    // Handle simple arithmetic expressions
    // Pattern: i OP number (where OP is +, -, *, /)
    if expr_str.contains(variable) {
        // Replace variable with its value
        let with_value = expr_str.replace(variable, &value.to_string());

        // Evaluate the expression
        // For safety, we'll use a simple evaluator for basic arithmetic
        match evaluate_simple_arithmetic(&with_value) {
            Ok(result) => {
                if result < 0 {
                    return Err(IrError::InvalidExpression(format!(
                        "Anchor index expression '{}' evaluates to negative value {} (when {}={}). \
                         Hardware indices cannot be negative.",
                        expr_str, result, variable, value
                    )));
                }
                Ok(result as usize)
            }
            Err(e) => Err(IrError::InvalidExpression(format!(
                "Failed to evaluate anchor index expression '{}': {}",
                expr_str, e
            ))),
        }
    } else {
        Err(IrError::InvalidExpression(format!(
            "Anchor index expression '{}' does not contain loop variable '{}'",
            expr_str, variable
        )))
    }
}
