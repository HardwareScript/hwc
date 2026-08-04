//! Region definitions for floorplanning (v0.2.0)

use crate::ast::common::{Coordinate, Identifier};
use crate::ast::expression::Expression;
use crate::lexer::Span;
use serde::{Deserialize, Serialize};

/// Region definition for floorplanning
/// Example:
/// ```hw
/// region AnalogRegion:
///     at: space.bottom_left + [100um, 100um]
/// 
/// region DigitalRegion:
///     right_of: AnalogRegion with spacing: pdk.min_spacing * 10
///     align: top with AnalogRegion
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionDefinition {
    pub name: Identifier,
    pub anchor: Option<RegionAnchor>,
    pub constraints: Vec<RegionConstraint>,
    pub boundary: Option<RegionBoundary>,
    pub span: Span,
}

use crate::ast::expression::BinaryOperator;

/// Region anchor point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionAnchor {
    /// Absolute coordinate: `at: [x: 100um, y: 100um]`
    Absolute(Coordinate),
    /// Expression base with vector offset: `at: space.bottom_left + [pdk.edge_clearance, pdk.edge_clearance]`
    Offset {
        base: Expression,
        operator: BinaryOperator,
        offset: Coordinate,
    },
    /// Expression-based anchor: `at: space.bottom_left`
    Expression(Expression),
}

/// Relational constraint between regions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionConstraint {
    pub constraint_type: RegionConstraintType,
    pub target: Identifier,
    pub spacing: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionConstraintType {
    RightOf,
    LeftOf,
    Above,
    Below,
    AlignTop,
    AlignBottom,
    AlignLeft,
    AlignRight,
    AlignX,
    AlignY,
}

/// Optional explicit boundary for a region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionBoundary {
    pub width: Expression,
    pub height: Expression,
    pub span: Span,
}
