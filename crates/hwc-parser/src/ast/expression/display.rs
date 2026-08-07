//! `Display` rendering of an [`Expression`] back to source-like form.

use std::fmt;

use super::types::Expression;
use crate::ast::Edge;

/// Source spelling of a bounding-box edge (`M1.right`, `Pad.center_x`, ...).
fn edge_str(edge: &Edge) -> &'static str {
    match edge {
        Edge::Left => "left",
        Edge::Right => "right",
        Edge::Top => "top",
        Edge::Bottom => "bottom",
        Edge::Front => "front",
        Edge::Back => "back",
        Edge::MinZ => "min_z",
        Edge::MaxZ => "max_z",
        Edge::TopLeft => "top_left",
        Edge::TopRight => "top_right",
        Edge::BottomLeft => "bottom_left",
        Edge::BottomRight => "bottom_right",
        Edge::Center => "center",
        Edge::CenterX => "center_x",
        Edge::CenterY => "center_y",
        Edge::CenterZ => "center_z",
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expression::Literal { value, .. } => write!(f, "{}", value),
            Expression::FloatLiteral { value, .. } => write!(f, "{}", value),
            Expression::Measurement { value, unit, .. } => write!(f, "{}{:?}", value, unit),
            Expression::Percentage { value, .. } => write!(f, "{}%", value),
            Expression::Variable { name, .. } => write!(f, "{}", name),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => write!(f, "{} {} {}", left, operator.as_str(), right),
            Expression::Unary {
                operator, operand, ..
            } => write!(f, "{}{}", operator.as_str(), operand),
            Expression::Grouped { expression, .. } => write!(f, "({})", expression),
            Expression::AnchorReference { anchor, edge, .. } => {
                write!(f, "{}.{}", anchor.name, edge_str(edge))
            }
            Expression::Coordinate { coord, .. } => {
                write!(f, "{:?}", coord) // Use debug format for coordinate
            }
            Expression::FunctionCall {
                name, arguments, ..
            } => {
                write!(f, "{}(", name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
        }
    }
}
