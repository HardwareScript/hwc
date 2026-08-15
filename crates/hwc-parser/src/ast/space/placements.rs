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
    /// Position coordinate when using `at:` (can be relative, declarative, or positional).
    pub position: Option<Coordinate>,
    /// Width and height when using `dimensions:` with `at:` syntax.
    /// When present with position, boundary is derived from position + dimensions by the compiler.
    pub width: Option<Expression>,
    pub height: Option<Expression>,
    /// Explicit boundary when using `boundary:` or `spanning:` syntax.
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

/// Binding priority for device terminal assignments (v0.2.2)
/// 
/// Determines the processing order when multiple pours bind to the same device terminal.
/// Lower priority pours are processed first, higher priority pours override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BindingPriority {
    /// Channel body pours (e.g., resistor body, transistor channel) - processed first
    /// Typically multi-terminal bindings that span between contacts
    Channel = 0,
    
    /// Contact head pours (e.g., terminal contacts, gate contacts) - processed last, override channel
    /// Typically single-terminal bindings that provide precise electrical connection points
    Contact = 100,
}

impl BindingPriority {
    /// Infer priority from terminal count - multi-terminal = Channel, single-terminal = Contact
    pub fn infer_from_terminals(terminals: &[CompactString]) -> Self {
        match terminals.len() {
            0 | 1 => Self::Contact,  // Single or no terminal = contact
            _ => Self::Channel,       // Multi-terminal = channel body
        }
    }
}

impl Default for BindingPriority {
    fn default() -> Self {
        Self::Contact  // Default to contact (safer - won't be overridden)
    }
}

/// Device binding for explicit intent-based extraction (Phase 4: Silent Atom)
/// 
/// Supports binding multiple terminals to a single pour:
/// - Single terminal: `device: M1.gate`
/// - Multiple terminals: `device: R1.A, R1.B` (both terminals on same pour)
/// 
/// v0.2.2: Added priority field for deterministic terminal assignment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBinding {
    pub device_name: CompactString,
    pub terminals: Vec<CompactString>, // Changed from single terminal to Vec<terminals>
    pub priority: BindingPriority,     // v0.2.2: Explicit priority for processing order
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

/// Contact/Via placement: `add contact(Tungsten) named Via_A at [x:500um, y:325um] spanning layer: l1 to l2`
/// or with relational positioning: `add contact(Tungsten) named Via_A at: Region.center spanning layer: l1 to l2`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactPlacement {
    pub material: CompactString,
    pub name: crate::ast::component::ComponentName, // v0.2.0: Required - no unnamed contacts
    pub position: Option<Coordinate>,
    pub from_elevation: Elevation,
    pub to_elevation: Elevation,
    pub net: Option<NetName>,
    pub properties: rustc_hash::FxHashMap<CompactString, Expression>,
    /// v0.2.1: Relational constraints (align, above, below, etc.)
    pub relational_constraints:
        smallvec::SmallVec<[crate::ast::component::RelationalConstraint; 2]>,
    #[serde(skip)]
    pub contour: Option<clipper2_rust::Path64>,
    pub span: Span,
}

/// Hierarchical sub-space instantiation (v0.2.1)
/// Example: `add space PMOS_Cell named PMOS_Inst at [x: 0nm, y: 0nm] rotated 0deg:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceInstancePlacement {
    /// The space definition being instantiated (e.g., "PMOS_Cell")
    pub space_name: crate::ast::common::Identifier,
    /// Instance name in the parent space (e.g., "PMOS_Inst")
    pub instance_name: crate::ast::component::ComponentName,
    /// Position in the parent coordinate system
    pub position: Coordinate,
    /// Optional rotation (0deg, 90deg, 180deg, 270deg)
    pub rotation: Option<crate::ast::common::Rotation>,
    /// Maps child space's local net names to parent space's net names
    /// e.g., "VDD_Rail" -> "VDD", "Out_Pad" -> "Out"
    pub net_map: rustc_hash::FxHashMap<CompactString, CompactString>,
    pub span: Span,
}
