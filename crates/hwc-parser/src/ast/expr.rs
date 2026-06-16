use serde::{Deserialize, Serialize};

/// A mathematical expression node in the AST.
///
/// Used by geometry blocks (Mode B) to represent expressions that are
/// evaluated at compile time to produce shape coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// A numeric literal: 42, 3.14
    Literal(f64),
    /// A variable or parameter reference: width, i, angle
    Identifier(String),
    /// Unary operation: -expr, +expr
    UnaryOp { op: UnaryOp, expr: Box<Expr> },
    /// Binary operation: expr + expr, expr * expr, etc.
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Function call: sin(expr), cos(expr), tan(expr)
    Call { name: String, args: Vec<Expr> },
    /// Conditional: if cond: then_expr else: else_expr
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    /// Modulo comparison used in conditions: a mod b = c
    /// Stored as a special node because it's common in geometry blocks.
    ModEquals {
        dividend: Box<Expr>,
        divisor: Box<Expr>,
        remainder: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Pos,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
}
