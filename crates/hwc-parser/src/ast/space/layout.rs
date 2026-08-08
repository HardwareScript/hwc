use crate::ast::common::Coordinate;
use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Layout block for mapping module internals: `layout ModuleName:`
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleLayoutBlock<'ast> {
    pub module_instance: CompactString,
    pub statements: Vec<LayoutStatement<'ast>>,
    pub span: Span,
}

/// Statement inside a layout block (mirrors ModuleStatement but for physical placement)
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutStatement<'ast> {
    Placement(&'ast ModuleInternalPlacement),
    For {
        variable: CompactString,
        start: usize,
        end: usize,
        body: Vec<LayoutStatement<'ast>>,
        span: Span,
    },
    If {
        condition: crate::ast::module::Condition,
        then_body: Vec<LayoutStatement<'ast>>,
        else_body: Option<Vec<LayoutStatement<'ast>>>,
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
