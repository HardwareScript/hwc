use super::elevation::Elevation;
use super::routes::NetName;
use crate::ast::common::Coordinate;
use crate::ast::expression::Expression;
use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Copper pour: `add pour(Copper) named GND_Plane on layer: l1:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PourPlacement {
    pub material: CompactString,
    pub name: crate::ast::component::ComponentName,
    pub elevation: Elevation,
    pub thickness: Option<Expression>,
    pub boundary: Option<PourBoundary>,
    pub net: Option<NetName>,
    pub device: Option<DeviceBinding>,
    pub thermal_relief: bool,
    pub waivers: crate::ast::common::Waivers,
    pub relational_constraints: SmallVec<[crate::RelationalConstraint; 2]>,
    pub inside_region: Option<crate::ast::common::Identifier>, // v0.2.0: Region containment
    pub span: Span,
}

/// Pour boundary shape: rectangle or circle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PourBoundary {
    Rect(Box<Coordinate>, Box<Coordinate>),
    Circle {
        center: Box<Coordinate>,
        radius: crate::Expression,
    },
}

/// Device binding for explicit intent-based extraction (Phase 4: Silent Atom)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBinding {
    pub device_name: CompactString,
    pub terminal: CompactString,
    pub span: Span,
}

/// Cutout shape for substrate and plane cutouts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CutoutShape {
    Rectangle {
        width: Expression,
        height: Expression,
        at: Coordinate,
    },
    Circle {
        radius: Expression,
        at: Coordinate,
    },
}

/// Plane placement: `add plane(Material) named Name on layer: <layer>: ...`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanePlacement {
    pub material: CompactString,
    pub name: crate::ast::component::ComponentName,
    pub shape: Option<ShapeInstance>,
    pub elevation: Elevation,
    pub thickness: Option<Expression>,
    pub from: Option<Coordinate>,
    pub to: Option<Coordinate>,
    pub net: Option<NetName>,
    pub cutouts: Vec<CutoutShape>,
    pub relational_constraints: SmallVec<[crate::RelationalConstraint; 2]>,
    pub inside_region: Option<crate::ast::common::Identifier>, // v0.2.0: Region containment
    pub span: Span,
}

/// Shape instance with parameters (v0.1.9)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeInstance {
    pub shape_name: CompactString,
    pub parameters: SmallVec<[crate::ast::component::Parameter; 4]>,
    pub span: Span,
}

/// Custom polygon: `add polygon(Copper) named WiFi_Antenna at [x:10, y:10, z:1]:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolygonPlacement {
    pub material: CompactString,
    pub name: crate::ast::component::ComponentName,
    pub position: Coordinate,
    pub points: SmallVec<[(f64, f64); 8]>,
    pub span: Span,
}

/// Type of cap for tube shapes (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapType {
    None,
    Annular,
    Solid,
}

/// Contact/Via placement: `add contact(Tungsten) at [x:500um, y:325um] spanning layer: l1 to l2`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactPlacement {
    pub material: CompactString,
    pub name: Option<crate::ast::component::ComponentName>,
    pub position: Coordinate,
    pub from_elevation: Elevation,
    pub to_elevation: Elevation,
    pub net: Option<NetName>,
    pub properties: rustc_hash::FxHashMap<CompactString, Expression>,
    #[serde(skip)]
    pub contour: Option<clipper2_rust::Path64>,
    pub span: Span,
}
