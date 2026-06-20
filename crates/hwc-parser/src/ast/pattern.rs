//! Pattern definition types for routing

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement};
use super::expression::Expression;
use crate::lexer::Span;
use compact_str::CompactString;

/// Pattern definition: `pattern Name (params):` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternDefinition {
    pub name: Identifier,
    pub params: Vec<PatternParameter>,
    pub strategy_goal: Option<CompactString>,
    pub steps: Vec<PatternStep>,
    pub span: Span,
}

/// Pattern parameter: `gap: Measurement`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternParameter {
    pub name: CompactString,
    pub param_type: ParameterType,
    pub span: Span,
}

/// Parameter type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterType {
    Measurement,
    Number,
    String,
}

/// Pattern step: `gap r 45` or `amp * 2 r 90`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternStep {
    pub distance: Expression,
    pub angle: Expression,
    pub span: Span,
}

/// Strategy definition: `strategy Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategyDefinition {
    pub name: Identifier,
    pub target: Option<StrategyTarget>,
    pub tolerance: Option<Measurement>,
    pub pattern: Option<PatternInstantiation>,
    pub span: Span,
}

/// Strategy target: match_longest, match_shortest, or specific length
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StrategyTarget {
    MatchLongest,
    MatchShortest,
    Specific(Measurement),
}

/// Pattern instantiation: `Zigzag(gap: 0.5mm)`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternInstantiation {
    pub name: CompactString,
    pub arguments: Vec<PatternArgument>,
    pub span: Span,
}

/// Pattern argument: `gap: 0.5mm`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternArgument {
    pub name: CompactString,
    pub value: Expression,
    pub span: Span,
}
