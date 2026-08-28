//! HardwareScript v0.3.0 Statement and Control Flow AST nodes

use super::expression::Expression;
use super::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// HardwareScript v0.3.0 Type Expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    /// Named type: Int, Float, Measurement, Point2D, Net, String, etc.
    Named {
        name: CompactString,
        type_args: Vec<TypeExpr>,
        span: Span,
    },
    /// Tuple type: (Type1, Type2)
    Tuple {
        elements: Vec<TypeExpr>,
        span: Span,
    },
    /// Function type: fn(Arg1, Arg2) -> ReturnType
    Function {
        params: Vec<TypeExpr>,
        return_type: Option<Box<TypeExpr>>,
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. }
            | TypeExpr::Tuple { span, .. }
            | TypeExpr::Function { span, .. } => *span,
        }
    }
}

/// Block of statements delimited by `{ ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub tail_expr: Option<Box<Expression>>,
    pub span: Span,
}

/// Binding pattern in `let` declarations: `x` or `(a, b, ...)`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingPattern {
    /// Single variable binding: `let x = ...`
    Identifier(CompactString),
    /// Multi-variable / tuple destructuring pattern: `let (a, b) = ...`
    Tuple(Vec<CompactString>),
}

/// Pattern in match arms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// Matches a specific value or enum variant: `TapType.P_Sub`
    Expr(Expression),
    /// Wildcard fallback: `_`
    Wildcard { span: Span },
}

/// Match arm: `pattern => { body }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
    pub span: Span,
}

/// Statement inside a function or block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    /// Let binding: `let (mut)? pattern (: Type)? = expr;`
    Let {
        mutable: bool,
        pattern: BindingPattern,
        type_annotation: Option<TypeExpr>,
        value: Expression,
        span: Span,
    },

    /// Assignment: `target (= | += | -= | *= | /= | %=) expr;`
    Assignment {
        target: Expression,
        operator: AssignmentOperator,
        value: Expression,
        span: Span,
    },

    /// If conditional: `if cond { ... } else { ... }`
    If {
        condition: Expression,
        then_block: Block,
        else_branch: Option<ElseBranch>,
        span: Span,
    },

    /// For loop: `for i in 0..num_vias { ... }` or `for k, v in items { ... }`
    For {
        variables: Vec<CompactString>,
        iterable: Expression,
        body: Block,
        span: Span,
    },

    /// Break statement: `break;`
    Break {
        span: Span,
    },

    /// Continue statement: `continue;`
    Continue {
        span: Span,
    },

    /// Match statement: `match target { pattern => { ... }, ... }`
    Match {
        target: Expression,
        arms: Vec<MatchArm>,
        span: Span,
    },

    /// Return statement: `return (expr)?;`
    Return {
        value: Option<Expression>,
        span: Span,
    },

    /// Assert statement: `assert(cond, "message");`
    Assert {
        condition: Expression,
        message: Option<String>,
        args: Vec<Expression>,
        span: Span,
    },

    /// Standalone expression statement: `println(...)` or `space.add_polygon(...)`
    Expression {
        expression: Expression,
        span: Span,
    },

    /// Route statement: `route from to { ... }`
    Route {
        from: Expression,
        to: Expression,
        intent: Option<CompactString>,
        body: Option<Block>,
        span: Span,
    },
}

/// Assignment operators: `=`, `+=`, `-=`, `*=`, `/=`, `%=`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssignmentOperator {
    Assign,        // =
    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=
}

/// Else branch of an if statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElseBranch {
    ElseIf(Box<Statement>),
    Block(Block),
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Let { span, .. }
            | Statement::Assignment { span, .. }
            | Statement::If { span, .. }
            | Statement::For { span, .. }
            | Statement::Break { span, .. }
            | Statement::Continue { span, .. }
            | Statement::Match { span, .. }
            | Statement::Return { span, .. }
            | Statement::Assert { span, .. }
            | Statement::Expression { span, .. }
            | Statement::Route { span, .. } => *span,
        }
    }
}
