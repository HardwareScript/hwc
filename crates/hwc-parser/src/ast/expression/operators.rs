//! Binary and unary operators plus their integer semantics.
//!
//! Floating-point semantics for the same operators live in `arithmetic.rs`
//! (`apply_op_f64`), so that integer and float behavior stay side by side with
//! the code that dispatches between them.

use serde::{Deserialize, Serialize};

/// Binary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Arithmetic operators
    Add,      // +
    Subtract, // -
    Multiply, // *
    Divide,   // /
    Modulo,   // % (via 'mod' keyword)

    // Comparison operators (v0.2.1: for compile-time conditionals)
    Equal,              // == (requires double equals for comparison)
    NotEqual,           // !=
    LessThan,           // <
    GreaterThan,        // >
    LessThanOrEqual,    // <=
    GreaterThanOrEqual, // >=

    // Boolean operators (v0.2.1: for compile-time conditionals)
    And, // and (logical AND)
    Or,  // or (logical OR)
}

/// Unary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Negate, // - (arithmetic negation)
    Plus,   // + (arithmetic positive)
    Not,    // not (logical NOT) (v0.2.1)
}

impl BinaryOperator {
    /// Get the precedence of this operator (higher = tighter binding)
    /// Precedence levels (from lowest to highest):
    /// 0: Boolean operators (or)
    /// 1: Boolean operators (and)
    /// 2: Comparison operators (==, !=, <, >, <=, >=)
    /// 3: Addition and subtraction (+, -)
    /// 4: Multiplication, division, modulo (*, /, mod)
    pub fn precedence(&self) -> u8 {
        match self {
            // Boolean OR (lowest precedence - evaluates last)
            BinaryOperator::Or => 0,
            // Boolean AND (higher than OR)
            BinaryOperator::And => 1,
            // Comparison operators
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::LessThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::LessThanOrEqual
            | BinaryOperator::GreaterThanOrEqual => 2,
            // Addition and subtraction
            BinaryOperator::Add | BinaryOperator::Subtract => 3,
            // Multiplication, division, modulo (highest precedence)
            BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Modulo => 4,
        }
    }

    /// Returns true if this operator produces a boolean (0/1) result rather
    /// than a value in the same dimension as its operands.
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Equal
                | BinaryOperator::NotEqual
                | BinaryOperator::LessThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::LessThanOrEqual
                | BinaryOperator::GreaterThanOrEqual
        )
    }

    /// Human-readable source form of this operator (used by `Display`).
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::Modulo => "%",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::LessThan => "<",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::LessThanOrEqual => "<=",
            BinaryOperator::GreaterThanOrEqual => ">=",
            BinaryOperator::And => "and",
            BinaryOperator::Or => "or",
        }
    }

    /// Apply this operator to two values
    /// Returns an error if the operation is invalid (overflow, division by zero, etc.)
    pub fn apply(&self, left: i64, right: i64) -> Result<i64, String> {
        match self {
            BinaryOperator::Add => left
                .checked_add(right)
                .ok_or("Integer overflow in addition"),
            BinaryOperator::Subtract => left
                .checked_sub(right)
                .ok_or("Integer overflow in subtraction"),
            BinaryOperator::Multiply => left
                .checked_mul(right)
                .ok_or("Integer overflow in multiplication"),
            BinaryOperator::Divide => {
                if right == 0 {
                    Err("Division by zero")
                } else {
                    Ok(left / right)
                }
            }
            BinaryOperator::Modulo => {
                if right == 0 {
                    Err("Modulo by zero")
                } else {
                    Ok(left % right)
                }
            }
            // Comparison operators return 1 for true, 0 for false
            BinaryOperator::Equal => Ok(if left == right { 1 } else { 0 }),
            BinaryOperator::NotEqual => Ok(if left != right { 1 } else { 0 }),
            BinaryOperator::LessThan => Ok(if left < right { 1 } else { 0 }),
            BinaryOperator::GreaterThan => Ok(if left > right { 1 } else { 0 }),
            BinaryOperator::LessThanOrEqual => Ok(if left <= right { 1 } else { 0 }),
            BinaryOperator::GreaterThanOrEqual => Ok(if left >= right { 1 } else { 0 }),
            // Boolean operators (treat non-zero as true, zero as false)
            BinaryOperator::And => Ok(if left != 0 && right != 0 { 1 } else { 0 }),
            BinaryOperator::Or => Ok(if left != 0 || right != 0 { 1 } else { 0 }),
        }
        .map_err(|s| s.to_string())
    }
}

impl UnaryOperator {
    /// Human-readable source form of this operator (used by `Display`).
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOperator::Negate => "-",
            UnaryOperator::Plus => "+",
            UnaryOperator::Not => "not ",
        }
    }

    /// Apply this operator to a value
    pub fn apply(&self, value: i64) -> Result<i64, String> {
        match self {
            UnaryOperator::Negate => value.checked_neg().ok_or("Integer overflow in negation"),
            UnaryOperator::Plus => Ok(value),
            UnaryOperator::Not => Ok(if value == 0 { 1 } else { 0 }), // Logical NOT: !0 = 1, !non-zero = 0
        }
        .map_err(|s| s.to_string())
    }
}
