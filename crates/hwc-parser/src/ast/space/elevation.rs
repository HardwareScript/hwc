use serde::{Deserialize, Serialize};

use crate::ast::common::Identifier;
use crate::ast::expression::Expression;
use crate::lexer::Span;

/// Z-axis elevation (v0.1.7 Z-Axis Abstraction)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Elevation {
    Physical {
        start: Expression,
        end: Option<Expression>,
    },
    Semantic(Identifier),
    Relative,
}

impl Elevation {
    pub fn is_physical(&self) -> bool {
        matches!(self, Elevation::Physical { .. })
    }

    pub fn is_semantic(&self) -> bool {
        matches!(self, Elevation::Semantic(_))
    }

    pub fn is_relative(&self) -> bool {
        matches!(self, Elevation::Relative)
    }

    pub fn as_physical_start(&self) -> Option<&Expression> {
        match self {
            Elevation::Physical { start, .. } => Some(start),
            Elevation::Semantic(_) | Elevation::Relative => None,
        }
    }

    pub fn as_physical_end(&self) -> Option<&Expression> {
        match self {
            Elevation::Physical { end, .. } => end.as_ref(),
            Elevation::Semantic(_) | Elevation::Relative => None,
        }
    }

    pub fn as_semantic_layer(&self) -> Option<&Identifier> {
        match self {
            Elevation::Physical { .. } | Elevation::Relative => None,
            Elevation::Semantic(id) => Some(id),
        }
    }
}

/// Global routing configuration for a space (v0.1.7)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub mode: RoutingMode,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingMode {
    Mixed,
    ManualOnly,
}
