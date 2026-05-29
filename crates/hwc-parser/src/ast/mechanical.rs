//! Mechanical definition types

use serde::{Deserialize, Serialize};

use super::common::{Coordinate, Dimensions, Identifier, Measurement};
use crate::lexer::Span;

/// Mechanical definition: `mechanical Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MechanicalDefinition {
    pub name: Identifier,
    pub dimensions: Option<Dimensions>,
    pub mounting_holes: Vec<MountingHole>,
    pub keepouts: Vec<Keepout>,
    pub span: Span,
}

/// Mounting hole specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountingHole {
    pub position: Coordinate,
    pub diameter: Measurement,
    pub span: Span,
}

/// Keepout region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keepout {
    pub from: Coordinate,
    pub to: Coordinate,
    pub height: Option<Measurement>,
    pub span: Span,
}
