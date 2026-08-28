//! HardwareScript v0.3.0 Expression AST nodes

use super::operators::{BinaryOperator, UnaryOperator};
use crate::ast::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// HardwareScript v0.3.0 Expression AST Node
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// Integer literal: 42, 0x1F, 0b1010
    Literal { value: i64, span: Span },

    /// Float literal: 3.14, 1.68e-8
    FloatLiteral { value: f64, span: Span },

    /// Physical measurement literal: 10um, 1.8V, 20mA, 150nm
    Measurement {
        value: f64,
        unit: crate::ast::Unit,
        span: Span,
    },

    /// String literal: "hello" or interpolated "NODE_{i}"
    StringLiteral { value: String, span: Span },

    /// Boolean literal: true, false
    BooleanLiteral { value: bool, span: Span },

    /// Variable / Identifier reference: x, VDD, Out
    Variable { name: CompactString, span: Span },

    /// Array literal: [1.0um, 2.0um] or [a, b, c]
    ArrayLiteral {
        elements: Vec<Expression>,
        span: Span,
    },

    /// Struct instantiation: StructName { field: val, ... }
    StructInstance {
        name: CompactString,
        fields: Vec<FieldInit>,
        span: Span,
    },

    /// Binary operation: a + b, x and y, p == q
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },

    /// Unary operation: not cond, -x, +x
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
        span: Span,
    },

    /// Range formation: start..end or start..=end
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        inclusive: bool,
        span: Span,
    },

    /// Function or method call: sky130_nmos(name: "M1", W: 1um) or space.add_polygon(...)
    Call {
        callee: Box<Expression>,
        arguments: Vec<NamedOrPositionalArg>,
        span: Span,
    },

    /// Member / Field access: nmos.source, at.x, space.add_polygon
    FieldAccess {
        target: Box<Expression>,
        field: CompactString,
        span: Span,
    },

    /// Array index access: array[index]
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },

    /// Parenthesized expression: (a + b)
    Grouped {
        expression: Box<Expression>,
        span: Span,
    },

    /// Tuple expression: (a, b, c)
    Tuple {
        elements: Vec<Expression>,
        span: Span,
    },

    /// Array slice expression: array[start..end]
    Slice {
        target: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        inclusive: bool,
        span: Span,
    },

    /// If / else expression: `if cond { a } else { b }`
    If {
        condition: Box<Expression>,
        then_branch: crate::ast::Block,
        else_branch: Option<Box<ElseBranchExpr>>,
        span: Span,
    },

    /// Match expression: `match target { pattern => expr / block, ... }`
    Match {
        target: Box<Expression>,
        arms: Vec<MatchArmExpr>,
        span: Span,
    },

    /// Block expression: `{ stmt*; tail_expr }`
    Block {
        block: crate::ast::Block,
        span: Span,
    },
}

/// Else branch for expression-oriented `if`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElseBranchExpr {
    ElseIf(Expression),
    Block(crate::ast::Block),
}

/// Match arm in expression-oriented `match`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArmExpr {
    pub pattern: crate::ast::Pattern,
    pub body: MatchArmBody,
    pub span: Span,
}

/// Body of a match arm: either a single Expression or a Block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MatchArmBody {
    Expr(Expression),
    Block(crate::ast::Block),
}

/// Named or positional argument for function/method calls
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedOrPositionalArg {
    pub name: Option<CompactString>,
    pub value: Expression,
    pub span: Span,
}

/// Field initializer for struct instantiation: `field: value` or shorthand `field`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldInit {
    pub name: CompactString,
    pub value: Option<Expression>,
    pub span: Span,
}

impl Expression {
    /// Return the source span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expression::Literal { span, .. }
            | Expression::FloatLiteral { span, .. }
            | Expression::Measurement { span, .. }
            | Expression::StringLiteral { span, .. }
            | Expression::BooleanLiteral { span, .. }
            | Expression::Variable { span, .. }
            | Expression::ArrayLiteral { span, .. }
            | Expression::StructInstance { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Range { span, .. }
            | Expression::Call { span, .. }
            | Expression::FieldAccess { span, .. }
            | Expression::Index { span, .. }
            | Expression::Grouped { span, .. }
            | Expression::Tuple { span, .. }
            | Expression::Slice { span, .. }
            | Expression::If { span, .. }
            | Expression::Match { span, .. }
            | Expression::Block { span, .. } => *span,
        }
    }
}
