//! Component definition and placement types

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::common::{Coordinate, Identifier, Measurement, Rotation};
use crate::lexer::Span;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Component definition: `component Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDefinition {
    pub name: Identifier,
    pub parameters: SmallVec<[ComponentParameter; 4]>,
    pub metadata: Option<ComponentMetadata>,
    pub pins: SmallVec<[CompactString; 4]>,
    pub layout: Option<LayoutBlock>,
    pub electrical: Option<ElectricalBlock>,
    pub render: Option<RenderBlock>,
    pub implements: SmallVec<[super::polymorphic_interface::InterfaceImplementation; 2]>, // NEW: Interface implementations for v0.1.5
    pub span: Span,
}

/// Parameter in component definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentParameter {
    pub name: CompactString,
    pub param_type: CompactString, // "Measurement", "String", "Number", etc.
}

/// Component metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub manufacturer: Option<CompactString>,
    pub part_number: Option<CompactString>,
    pub package: Option<CompactString>,
    pub value: Option<CompactString>,
    pub description: Option<CompactString>,
    pub datasheet: Option<CompactString>,
    pub other: FxHashMap<CompactString, String>,
    pub span: Span,
}

/// Layout block (v0.1.4)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutBlock {
    pub shape: Option<CompactString>, // Shape type (e.g., "Rectangle", "Cylinder")
    pub pin_positions: FxHashMap<CompactString, PinPosition>,
    pub pad_shapes: FxHashMap<CompactString, String>, // Pin name -> pad shape (e.g., "Circle(0.5mm)")
    /// Internal geometry (v0.1.6 Sprint 2): Pours defined inside the component
    /// These are relative to the component's origin and get "unrolled" during placement
    pub internal_pours: Vec<super::space::PourPlacement>,
    pub span: Span,
}

/// Pin position in layout (stored in millimeters)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinPosition {
    /// X coordinate in millimeters
    pub x: f64,
    /// Y coordinate in millimeters
    pub y: f64,
    /// Z coordinate in millimeters (optional)
    pub z: Option<f64>,
}

/// Electrical block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectricalBlock {
    pub properties: FxHashMap<CompactString, String>,
}

/// Render block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderBlock {
    pub render_type: Option<CompactString>, // "procedural" or "asset"
    pub shape: Option<CompactString>,
    pub body_color: Option<CompactString>,
    pub endcap_color: Option<CompactString>,
    pub label: Option<CompactString>,
    pub asset: Option<CompactString>,
    pub view: Option<CompactString>, // NEW v0.1.6: Orientation hint (e.g., "horizontal", "vertical")
}

/// Component placement: `add Type (params) named Instance at [X,Y,Z] rotated angle`
/// v0.1.6 Sprint 3.2: Supports array syntax: `add Type[count] named ArrayName`
/// v0.1.6 Sprint 3.4: Supports indexed names in loops: `named Adder[i]`
/// v0.1.6 GAP2: Supports `allow_substrate_overlap` attribute for embedded components
/// v0.1.6 Sprint 3.2: Supports `skip_collision_check` for merged arrays
/// v0.1.6 Item #13: Supports `net:` block for pin-to-net binding
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentPlacement {
    pub component_type: Identifier,
    pub parameters: SmallVec<[Parameter; 4]>,
    pub name: Option<ComponentName>,
    pub position: Coordinate,
    pub rotation: Option<Rotation>,
    /// Z elevation (v0.1.7): either physical `z: 150um` or semantic `layer: l1`
    pub elevation: Option<super::space::Elevation>,
    /// Array configuration (v0.1.6 Sprint 3.2)
    pub array_config: Option<ArrayConfig>,
    /// Pin-to-net bindings (v0.1.6 Item #13)
    /// Maps component pin names to net names
    /// Example: {"a": "A[i]", "b": "B[i]", "carry_in": "Carry[i-1]"}
    pub pin_net_bindings: FxHashMap<CompactString, NetBinding>,
    /// Intentional design waivers (v0.1.6 Sprint 8)
    ///
    /// Replaces legacy `allow_substrate_overlap` and `skip_collision_check` flags.
    /// Acts as the central "Intent" registry for this component instance.
    pub waivers: super::common::Waivers,
    pub span: Span,
}

/// Net binding for a component pin (v0.1.6 Item #13)
/// Can be a simple net name or a conditional expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NetBinding {
    /// Simple net name: "VDD" or "A[i]"
    Simple(CompactString),
    /// Conditional expression: if i == 0 then "CarryIn" else "Carry[i-1]"
    Conditional {
        condition: super::Expression,
        then_net: CompactString,
        else_net: CompactString,
    },
}

/// Component name with optional array index (v0.1.6 Sprint 3.4)
/// Supports both simple names (`M1`) and indexed names (`Adder[i]`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentName {
    pub base: CompactString,
    pub index: Option<super::Expression>,
    pub span: Span,
}

impl ComponentName {
    /// Create a simple component name without an index
    pub fn simple(name: CompactString, span: Span) -> Self {
        ComponentName {
            base: name,
            index: None,
            span,
        }
    }

    /// Create an indexed component name
    pub fn indexed(name: CompactString, index: super::Expression, span: Span) -> Self {
        ComponentName {
            base: name,
            index: Some(index),
            span,
        }
    }

    /// Convert to string representation (for display/debugging)
    pub fn to_string(&self) -> CompactString {
        if let Some(ref idx) = self.index {
            format!("{}[{}]", self.base, idx).into()
        } else {
            self.base.clone()
        }
    }

    /// Get the base name without index
    pub fn base_name(&self) -> &str {
        &self.base
    }

    /// Get the base name as a string slice (for compatibility)
    pub fn as_str(&self) -> &str {
        &self.base
    }
}

impl std::fmt::Display for ComponentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref idx) = self.index {
            write!(f, "{}[{}]", self.base, idx)
        } else {
            write!(f, "{}", self.base)
        }
    }
}

/// Array configuration for component arrays (v0.1.6 Sprint 3.2)
/// Syntax: `add ComponentType[count] named ArrayName`
/// Example: `add NMOS[4] named M1_Array layout: horizontal_stack pitch: 2um merge: [source, drain]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrayConfig {
    /// Number of instances in the array
    pub count: usize,
    /// Layout strategy: horizontal_stack, vertical_stack, grid
    pub layout: ArrayLayout,
    /// Spacing between instances (center-to-center)
    pub pitch: Measurement,
    /// Terminals that should be explicitly merged when overlapping (EXPLICIT INTENT)
    ///
    /// **Philosophy**: NO IMPLICIT MAGIC (Hardware Script Manifesto)
    /// - Without `merge:` → Overlapping geometry triggers P12: Geometric Collision Error
    /// - With `merge: [source, drain]` → Compiler performs Bitwise-OR melting (explicit intent)
    ///
    /// Example: `merge: [source, drain]` means "I know these overlap. Melt them."
    pub merge_terminals: SmallVec<[CompactString; 2]>,
    pub span: Span,
}

/// Array layout strategy (v0.1.6 Sprint 3.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrayLayout {
    /// Horizontal stack (X-axis)
    HorizontalStack,
    /// Vertical stack (Y-axis)
    VerticalStack,
    /// 2D grid (future enhancement)
    Grid { rows: usize, cols: usize },
}

/// Parameter value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Measurement(Measurement),
    String(String),
    Number(f64),
}

/// Parameter in component instantiation
/// v0.1.6: Only keyword arguments supported
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Parameter {
    /// Keyword parameter: `(val: 10kΩ)` or `(color: "Red")` or `(count: 8)`
    Keyword {
        name: CompactString,
        value: ParameterValue,
    },
}
