//! Logic synthesis AST for v0.4.0
//!
//! Implements logic blocks that describe hardware using Rust-like syntax.
//! These blocks are expanded into component placements during compilation.
//!
//! Reference: Logic Synthesis Specification v0.4.0

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use super::Span;

/// Logic definition: `logic name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub logic_block: LogicBlock,
    pub span: Span,
}

/// Enum definition: `enum Name: Value1, Value2 = 0x1` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// Enum variant with optional explicit value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: CompactString,
    pub value: Option<i64>, // Explicit value like Add = 0x1
    pub span: Span,
}

/// Struct definition: `struct Name: field1[8], field2[4]` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// Struct field with bit width
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub name: CompactString,
    pub width: usize,
    pub span: Span,
}

/// Logic block: `logic:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicBlock {
    pub statements: Vec<LogicStatement>,
    pub span: Span,
}

/// Statement inside a logic block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogicStatement {
    /// Let declaration: `let x = A + B` or `let mut result = 0`
    Let {
        mutable: bool,
        name: CompactString,
        width: Option<usize>, // For `let x[16] = ...`
        expression: LogicExpression,
        span: Span,
    },

    /// Assignment: `result = A + B` or `state.next = Value`
    Assignment {
        target: AssignmentTarget,
        expression: LogicExpression,
        span: Span,
    },

    /// If statement: `if condition: ...`
    If {
        condition: LogicExpression,
        then_block: BlockOrExpr,
        else_block: Option<BlockOrExpr>,
        span: Span,
    },

    /// A bare expression evaluated for its physical value (tail expression in a block)
    /// This allows blocks to return values naturally, like Rust
    Expression(LogicExpression),
}

/// Target of an assignment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssignmentTarget {
    /// Simple variable: `result`
    Variable { name: CompactString, span: Span },

    /// Register next state: `state.next`
    RegisterNext { name: CompactString, span: Span },

    /// Array slice: `Bus[7..0]`
    Slice {
        name: CompactString,
        range: Range,
        span: Span,
    },
}

/// Block or single expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockOrExpr {
    /// Single expression
    Expression(LogicExpression),

    /// Pass (empty block)
    Pass(Span),

    /// Block of statements
    Block(Vec<LogicStatement>),
}

/// Range specification for bus operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Range {
    /// Single bit: Bus[5]
    Single(usize),
    /// Bit range: Bus[7..0] or Bus[7..=0]
    /// In hardware contexts, bit slices are typically inclusive (e.g., [7:0] in Verilog)
    /// The `inclusive` flag determines iteration behavior
    Slice { 
        high: usize, 
        low: usize,
        /// true for ..= (inclusive), false for .. (exclusive)
        inclusive: bool,
    },
}

/// Expression in logic context
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogicExpression {
    /// Variable reference: `A`, `state`
    Variable { name: CompactString, span: Span },

    /// Field access: `instr.opcode`, `state.next`
    FieldAccess {
        base: Box<LogicExpression>,
        field: CompactString,
        span: Span,
    },

    /// Array access: `Bus[7]`, `Bus[7..0]`
    ArrayAccess {
        base: Box<LogicExpression>,
        range: Range,
        span: Span,
    },

    /// Integer literal: `42`, `0xFF`
    Literal { value: i64, span: Span },

    /// Boolean literal: `true`, `false`
    Boolean { value: bool, span: Span },

    /// Binary operation: `A + B`, `X & Y`, `P == Q`
    Binary {
        left: Box<LogicExpression>,
        operator: LogicOperator,
        right: Box<LogicExpression>,
        span: Span,
    },

    /// Unary operation: `!A`, `not Enable`
    Unary {
        operator: LogicUnaryOperator,
        operand: Box<LogicExpression>,
        span: Span,
    },

    /// Parenthesized expression: `(A + B)`
    Grouped {
        expression: Box<LogicExpression>,
        span: Span,
    },

    /// Match expression: `match OpCode: 0x0: A, 0x1: B, else: 0`
    Match {
        selector: Box<LogicExpression>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    /// If expression: `if Enable: A else: B` or multi-line with blocks
    If {
        condition: Box<LogicExpression>,
        then_expr: Box<BlockOrExpr>,
        else_expr: Box<BlockOrExpr>,
        span: Span,
    },

    /// Type cast: `RawInstr as Instruction`
    Cast {
        expression: Box<LogicExpression>,
        target_type: CompactString,
        span: Span,
    },

    /// Register initialization: `reg(clock: Clk, reset: Rst, init: 0)`
    RegisterInit {
        clock: Box<LogicExpression>,
        reset: Box<LogicExpression>,
        init: Box<LogicExpression>,
        span: Span,
    },

    /// Bundle/Concatenation: `[A[8], B[8]]`
    Bundle { items: Vec<BundleItem>, span: Span },
}

/// Item in a bundle expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BundleItem {
    /// Single expression: `A[8]`
    Expression(LogicExpression),

    /// Duplication: `(0 * 12)` - duplicates value N times
    Duplication {
        value: Box<LogicExpression>,
        count: usize,
        span: Span,
    },
}

/// Match arm in a match expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: BlockOrExpr,
    pub span: Span,
}

/// Pattern in a match arm
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MatchPattern {
    /// Literal value: `0x0`, `42`
    Literal(i64),
    /// Enum variant: `CpuState.Fetch`
    EnumVariant {
        enum_name: CompactString,
        variant: String,
    },
    /// Default/else: `else:`
    Else,
}

/// Binary operators for logic expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicOperator {
    // Arithmetic
    Add,      // +
    Subtract, // -
    Multiply, // *
    Divide,   // /
    Modulo,   // mod (v0.1.6: keyword-only, % reserved for units)

    // Bitwise
    BitwiseAnd, // & or 'and'
    BitwiseOr,  // | or 'or'
    BitwiseXor, // 'xor' (word-only in v0.1.6, no ^ symbol)
    ShiftLeft,  // <<
    ShiftRight, // >>

    // Comparison
    Equal,              // = (v0.1.6: context-aware, single equals for comparison)
    NotEqual,           // !=
    LessThan,           // <
    GreaterThan,        // >
    LessThanOrEqual,    // <=
    GreaterThanOrEqual, // >=
}

/// Unary operators for logic expressions (v0.1.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicUnaryOperator {
    /// Logical/Bitwise NOT: `!` or `not`
    /// In hardware: synthesizes to an inverter gate
    Not,
}

impl LogicUnaryOperator {
    /// Get the operator symbol as a string
    pub fn symbol(&self) -> &'static str {
        match self {
            LogicUnaryOperator::Not => "!",
        }
    }
}

impl LogicOperator {
    /// Get the precedence of this operator (higher = tighter binding)
    pub fn precedence(&self) -> u8 {
        match self {
            // Multiplicative (highest)
            LogicOperator::Multiply | LogicOperator::Divide | LogicOperator::Modulo => 6,

            // Additive
            LogicOperator::Add | LogicOperator::Subtract => 5,

            // Shift
            LogicOperator::ShiftLeft | LogicOperator::ShiftRight => 4,

            // Comparison
            LogicOperator::LessThan
            | LogicOperator::GreaterThan
            | LogicOperator::LessThanOrEqual
            | LogicOperator::GreaterThanOrEqual => 3,

            // Equality
            LogicOperator::Equal | LogicOperator::NotEqual => 2,

            // Bitwise (lowest)
            LogicOperator::BitwiseAnd | LogicOperator::BitwiseOr | LogicOperator::BitwiseXor => 1,
        }
    }

    /// Get the operator symbol as a string
    pub fn symbol(&self) -> &'static str {
        match self {
            LogicOperator::Add => "+",
            LogicOperator::Subtract => "-",
            LogicOperator::Multiply => "*",
            LogicOperator::Divide => "/",
            LogicOperator::Modulo => "mod",
            LogicOperator::BitwiseAnd => "&",
            LogicOperator::BitwiseOr => "|",
            LogicOperator::BitwiseXor => "xor", // v0.1.6: word-only, no ^ symbol
            LogicOperator::ShiftLeft => "<<",
            LogicOperator::ShiftRight => ">>",
            LogicOperator::Equal => "=", // v0.1.6: single = for comparison (context-aware)
            LogicOperator::NotEqual => "!=",
            LogicOperator::LessThan => "<",
            LogicOperator::GreaterThan => ">",
            LogicOperator::LessThanOrEqual => "<=",
            LogicOperator::GreaterThanOrEqual => ">=",
        }
    }
}

impl LogicExpression {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            LogicExpression::Variable { span, .. }
            | LogicExpression::FieldAccess { span, .. }
            | LogicExpression::ArrayAccess { span, .. }
            | LogicExpression::Literal { span, .. }
            | LogicExpression::Boolean { span, .. }
            | LogicExpression::Binary { span, .. }
            | LogicExpression::Unary { span, .. }
            | LogicExpression::Grouped { span, .. }
            | LogicExpression::Match { span, .. }
            | LogicExpression::If { span, .. }
            | LogicExpression::Cast { span, .. }
            | LogicExpression::RegisterInit { span, .. }
            | LogicExpression::Bundle { span, .. } => *span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_precedence() {
        assert!(LogicOperator::Multiply.precedence() > LogicOperator::Add.precedence());
        assert!(LogicOperator::Add.precedence() > LogicOperator::Equal.precedence());
        assert!(LogicOperator::Equal.precedence() > LogicOperator::BitwiseAnd.precedence());
    }
}
