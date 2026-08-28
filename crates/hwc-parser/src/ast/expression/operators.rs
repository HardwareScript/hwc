//! Binary and unary operators for HardwareScript v0.3.0

use serde::{Deserialize, Serialize};

/// Binary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Logical
    Or,  // or (Level 1)
    And, // and (Level 2)

    // Bitwise (Level 3-5)
    BitwiseOr,  // |
    BitwiseXor, // ^
    BitwiseAnd, // &

    // Equality & Comparison (Level 6)
    Equal,              // ==
    NotEqual,           // !=
    LessThan,           // <
    GreaterThan,        // >
    LessThanOrEqual,    // <=
    GreaterThanOrEqual, // >=

    // Bit shifts (Level 7)
    ShiftLeft,  // <<
    ShiftRight, // >>

    // Additive (Level 8)
    Add,      // +
    Subtract, // -

    // Multiplicative (Level 9)
    Multiply, // *
    Divide,   // /
    Modulo,   // %
}

/// Unary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,        // not (Logical Not)
    Negate,     // - (Arithmetic Negation)
    Plus,       // + (Arithmetic Positive)
    BitwiseNot, // ~ (Bitwise Not)
}

impl BinaryOperator {
    /// Pratt precedence hierarchy (higher = tighter binding)
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOperator::Or => 1,
            BinaryOperator::And => 2,
            BinaryOperator::BitwiseOr => 3,
            BinaryOperator::BitwiseXor => 4,
            BinaryOperator::BitwiseAnd => 5,
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::LessThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::LessThanOrEqual
            | BinaryOperator::GreaterThanOrEqual => 6,
            BinaryOperator::ShiftLeft | BinaryOperator::ShiftRight => 7,
            BinaryOperator::Add | BinaryOperator::Subtract => 8,
            BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Modulo => 9,
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
            BinaryOperator::BitwiseOr => "|",
            BinaryOperator::BitwiseXor => "^",
            BinaryOperator::BitwiseAnd => "&",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::LessThan => "<",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::LessThanOrEqual => "<=",
            BinaryOperator::GreaterThanOrEqual => ">=",
            BinaryOperator::ShiftLeft => "<<",
            BinaryOperator::ShiftRight => ">>",
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
            UnaryOperator::BitwiseNot => "~",
        }
    }
}
