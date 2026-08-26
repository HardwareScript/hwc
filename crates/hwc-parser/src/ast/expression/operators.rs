//! Binary and unary operators for HardwareScript v0.3.0

use serde::{Deserialize, Serialize};

/// Binary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Logical
    Or,  // or (Level 1)
    And, // and (Level 2)

    // Equality & Comparison (Level 3)
    Equal,              // ==
    NotEqual,           // !=
    LessThan,           // <
    GreaterThan,        // >
    LessThanOrEqual,    // <=
    GreaterThanOrEqual, // >=

    // Additive (Level 5)
    Add,      // +
    Subtract, // -

    // Multiplicative (Level 6)
    Multiply, // *
    Divide,   // /
    Modulo,   // %
}

/// Unary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,    // not (Logical Not)
    Negate, // - (Arithmetic Negation)
    Plus,   // + (Arithmetic Positive)
}

impl BinaryOperator {
    /// Pratt precedence hierarchy (higher = tighter binding)
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOperator::Or => 1,
            BinaryOperator::And => 2,
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::LessThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::LessThanOrEqual
            | BinaryOperator::GreaterThanOrEqual => 3,
            BinaryOperator::Add | BinaryOperator::Subtract => 5,
            BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Modulo => 6,
        }
    }

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

    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOperator::Or => "or",
            BinaryOperator::And => "and",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::LessThan => "<",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::LessThanOrEqual => "<=",
            BinaryOperator::GreaterThanOrEqual => ">=",
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::Modulo => "%",
        }
    }
}

impl UnaryOperator {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOperator::Not => "not ",
            UnaryOperator::Negate => "-",
            UnaryOperator::Plus => "+",
        }
    }
}
