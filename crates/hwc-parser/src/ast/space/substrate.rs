use crate::ast::common::Coordinate;
use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Substrate placement: `add Substrate(FR4) spanning [1,1,1] to [4,500,500]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubstratePlacement {
    pub material: CompactString,
    pub from: Coordinate,
    pub to: Coordinate,
    pub cutouts: Vec<CoordinatePair>,
    pub span: Span,
}

/// A coordinate pair defining a bounding box region (for cutouts, keepouts, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoordinatePair {
    pub from: Coordinate,
    pub to: Coordinate,
}
