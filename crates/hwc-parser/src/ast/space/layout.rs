use crate::ast::arena::ModuleInternalId;
use crate::ast::common::Coordinate;
use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Layout block for mapping module internals: `layout ModuleName:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleLayoutBlock {
    pub module_instance: CompactString,
    pub statements: Vec<LayoutStatement>,
    pub span: Span,
}

/// Statement inside a layout block (mirrors ModuleStatement but for physical placement)
///
/// Range semantics (Rust/Swift-style explicit):
/// - `0..3` (exclusive): Iterates 3 times [0, 1, 2] - count-driven
/// - `0..=3` (inclusive): Iterates 4 times [0, 1, 2, 3] - bound-driven
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutStatement {
    Placement(ModuleInternalId),
    For {
        variable: CompactString,
        start: usize,
        end: usize,
        inclusive: bool,
        body: Vec<LayoutStatement>,
        span: Span,
    },
    If {
        condition: crate::ast::module::Condition,
        then_body: Vec<LayoutStatement>,
        else_body: Option<Vec<LayoutStatement>>,
        span: Span,
    },
}

/// Internal component placement within a module layout block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInternalPlacement {
    pub component_name: CompactString,
    pub array_index: Option<crate::ast::module::ArrayIndex>,
    pub position: Coordinate,
    pub span: Span,
}
