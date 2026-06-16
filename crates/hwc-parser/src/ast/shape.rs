use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use super::expr::Expr;
use crate::lexer::Span;

/// A 2D CSG expression (union, difference, intersection)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CsgExpression {
    /// A primitive shape: Rectangle, Circle, etc.
    Primitive(CsgPrimitive),
    /// Union of two shapes: A + B
    Union(Box<CsgExpression>, Box<CsgExpression>),
    /// Difference of two shapes: A - B
    Difference(Box<CsgExpression>, Box<CsgExpression>),
    /// Intersection of two shapes: A * B
    Intersection(Box<CsgExpression>, Box<CsgExpression>),
    /// Transformed shape: rotated, translated
    Transformed {
        expr: Box<CsgExpression>,
        rotation: Option<f64>,           // degrees
        translation: Option<(f64, f64)>, // x, y in nm
    },
    /// Let binding: let name = expr in body
    LetBinding {
        name: String,
        value: Box<CsgExpression>,
        body: Box<CsgExpression>,
    },
}

/// A primitive shape for CSG
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CsgPrimitive {
    Rectangle {
        width: String,
        height: String,
    },
    Circle {
        diameter: String,
    },
    /// Reference to a named shape definition
    ShapeRef(String),
}

/// A parameter for a shape definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeParameter {
    pub name: Identifier,
    pub default_value: Option<String>,
}

/// A 2D point in a shape definition (relative to center [0, 0])
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapePoint {
    pub x_expr: String,
    pub y_expr: String,
}

/// A procedural shape generator call (e.g., StarGenerator(points: 16, outer: width / 2, inner: width / 4))
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeGenerator {
    pub name: String,
    pub params: FxHashMap<String, String>,
}

/// A statement inside a geometry block's loop body
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeometryStatement {
    /// A variable declaration: let name = expr
    Variable { name: String, value: Expr },
    /// A point expression: Point(x: expr, y: expr)
    Point { x: Expr, y: Expr },
}

/// A geometry block in a shape definition (Mode B: Parametric Loop & Trigonometry)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeometryBlock {
    /// A for loop: for i in start..end: ...
    ForLoop {
        variable: String,
        start: i64,
        end: i64,
        body: Vec<GeometryStatement>,
    },
    /// A variable declaration at geometry block scope: let x = expr
    Variable { name: String, value: Expr },
}

/// A shape definition — defines a 2D polygon cross-section for vias
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeDefinition {
    pub name: Identifier,
    pub parameters: Vec<ShapeParameter>,
    pub points: Vec<ShapePoint>,
    pub generator: Option<ShapeGenerator>,
    pub geometry: Option<Vec<GeometryBlock>>,
    pub csg: Option<CsgExpression>, // NEW: CSG expression
    pub span: Span,
}
