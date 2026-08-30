//! Interpolated identifier parsing for template-style name generation
//!
//! v0.2.1: Added InterpolatedIdentifier for modern template-style name generation
//! Example: `L1_R{row}_C{col}` compiles to individual names at compile time



/// Part of an interpolated identifier - either literal text or an expression
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedPart {
    /// Literal text part: "L1_R", "_C", etc.
    Literal(String),
    /// Expression part (unparsed source text): "row", "col", "i+1", etc.
    /// Will be parsed into Expression AST later by the parser
    Expression(String),
}


