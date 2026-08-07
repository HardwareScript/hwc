//! Read-only queries over an [`Expression`] tree.

use super::types::Expression;
use crate::ast::{Coordinate, Span};

impl Expression {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expression::Literal { span, .. }
            | Expression::FloatLiteral { span, .. }
            | Expression::Measurement { span, .. }
            | Expression::Percentage { span, .. }
            | Expression::Variable { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Grouped { span, .. }
            | Expression::AnchorReference { span, .. }
            | Expression::Coordinate { span, .. }
            | Expression::FunctionCall { span, .. } => *span,
        }
    }

    /// Check if this expression is a simple literal
    pub fn as_literal(&self) -> Option<i64> {
        match self {
            Expression::Literal { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Check if this expression is a simple variable
    pub fn as_variable(&self) -> Option<&str> {
        match self {
            Expression::Variable { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    /// Check if this expression contains anchor references (needs constraint solving)
    pub fn contains_anchor_reference(&self) -> bool {
        match self {
            Expression::AnchorReference { .. } => true,
            Expression::Coordinate { coord, .. } => {
                // Check if the coordinate itself contains anchor references
                match coord.as_ref() {
                    Coordinate::Relative(_) => true,
                    Coordinate::Positional { x, y, z, .. } => {
                        x.contains_anchor_reference()
                            || y.contains_anchor_reference()
                            || z.contains_anchor_reference()
                    }
                    Coordinate::Declarative { x, y, z, .. } => {
                        x.contains_anchor_reference()
                            || y.contains_anchor_reference()
                            || z.contains_anchor_reference()
                    }
                }
            }
            Expression::Binary { left, right, .. } => {
                left.contains_anchor_reference() || right.contains_anchor_reference()
            }
            Expression::Unary { operand, .. } => operand.contains_anchor_reference(),
            Expression::Grouped { expression, .. } => expression.contains_anchor_reference(),
            Expression::FunctionCall { arguments, .. } => {
                arguments.iter().any(|arg| arg.contains_anchor_reference())
            }
            _ => false,
        }
    }
}
