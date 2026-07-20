use crate::ast::common::Coordinate;
use crate::ast::expression::Expression;
use crate::ast::pattern::PatternInstantiation;
use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Route endpoint specification in the parsed AST (v0.1.8)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RouteEndpointSpec {
    ComponentPin {
        component_name: CompactString,
        component_index: Option<Expression>,
        pin_name: CompactString,
        pin_index: Option<Expression>,
        span: Span,
    },
    SpaceEntity {
        name: CompactString,
        index: Option<Expression>,
        span: Span,
    },
}

impl RouteEndpointSpec {
    pub fn span(&self) -> Span {
        match self {
            RouteEndpointSpec::ComponentPin { span, .. } => *span,
            RouteEndpointSpec::SpaceEntity { span, .. } => *span,
        }
    }
}

/// Route: `route From.Pin to To.Pin:` with `path:` block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub from: RouteEndpointSpec,
    pub to: RouteEndpointSpec,
    pub width: Option<Expression>,
    pub layer: Option<crate::ast::common::Identifier>,
    pub strategy: Option<crate::ast::common::Identifier>,
    pub pattern: Option<PatternInstantiation>,
    pub strategy_params: Vec<(crate::ast::common::Identifier, Expression)>,
    pub path: Option<Vec<Coordinate>>,
    pub signal_group: Option<CompactString>,
    pub bridge: Option<CompactString>,
    pub exit_escape: Option<RouteEscape>,
    pub enter_escape: Option<RouteEscape>,
    pub current_limit_ac: Option<CurrentLimitAc>,
    pub span: Span,
}

/// Route escape specification for port-based routing (v0.1.7)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteEscape {
    pub port: CardinalDirection,
    pub offset: Option<EdgeOffsetSpec>,
    pub span: Span,
}

/// Cardinal direction for port escapes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

/// Edge offset specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeOffsetSpec {
    Named(NamedPosition),
    Percentage(f64),
    Measurement(i64),
}

/// Named positions for edge offsets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamedPosition {
    Top,
    Bottom,
    Center,
}

/// Expose: `expose Pin as Alias`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expose {
    pub pin: RouteEndpointSpec,
    pub alias: CompactString,
    pub span: Span,
}

/// Net name with optional array index (v0.1.6 Sprint 3.4)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetName {
    pub base: CompactString,
    pub index: Option<Expression>,
    pub span: Span,
}

impl NetName {
    pub fn simple(name: CompactString, span: Span) -> Self {
        NetName {
            base: name,
            index: None,
            span,
        }
    }

    pub fn indexed(name: CompactString, index: Expression, span: Span) -> Self {
        NetName {
            base: name,
            index: Some(index),
            span,
        }
    }

    pub fn to_string(&self) -> CompactString {
        if let Some(ref idx) = self.index {
            format!("{}[{}]", self.base, idx).into()
        } else {
            self.base.clone()
        }
    }

    pub fn base_name(&self) -> &str {
        &self.base
    }
}

impl std::fmt::Display for NetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref idx) = self.index {
            write!(f, "{}[{}]", self.base, idx)
        } else {
            write!(f, "{}", self.base)
        }
    }
}

/// AC current limit for route configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentLimitAc {
    pub rms: Expression,
    pub peak: Expression,
    pub span: Span,
}
