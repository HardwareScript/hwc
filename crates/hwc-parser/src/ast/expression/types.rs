//! The [`Expression`] AST node.
//!
//! This file intentionally contains data only. Behavior lives in the sibling
//! modules: `inspect` (queries), `eval` (evaluation) and `display` (formatting).

use super::operators::{BinaryOperator, UnaryOperator};
use crate::ast::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Mathematical expression that can be evaluated at compile time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// Integer literal: 42
    Literal { value: i64, span: Span },
    /// Float literal: 3.14 (v0.1.7)
    FloatLiteral { value: f64, span: Span },
    /// Measurement literal: 10mm, 5cm
    Measurement {
        value: f64,
        unit: crate::ast::Unit,
        span: Span,
    },
    /// Percentage literal: 50%, 25%
    Percentage { value: f64, span: Span },
    /// Variable reference: i, x, count
    Variable { name: CompactString, span: Span },
    /// Binary operation: a + b, x * 2, etc.
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },
    /// Unary operation: -x, +x
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
        span: Span,
    },
    /// Parenthesized expression: (x + 1)
    Grouped {
        expression: Box<Expression>,
        span: Span,
    },
    /// Anchor reference: ComponentName.edge (v0.1.6 Sprint 3.8)
    /// Allows mixing relative and absolute coordinates per axis
    /// Example: [x: GroundPlane.right + 1mm, y: 5mm, z: 2]
    AnchorReference {
        anchor: crate::ast::AnchorReference,
        edge: crate::ast::Edge,
        span: Span,
    },
    /// Coordinate literal as an expression (v0.2.0)
    /// Allows coordinate math: PMOS_Region.center - [200nm, 0nm]
    /// Example: at: anchor.center + [1mm, 2mm, 0mm]
    Coordinate {
        coord: Box<crate::ast::Coordinate>,
        span: Span,
    },
    /// Function call: sin(x), cos(angle), sqrt(value) (v0.2.1)
    FunctionCall {
        name: CompactString,
        arguments: Vec<Expression>,
        span: Span,
    },
}
