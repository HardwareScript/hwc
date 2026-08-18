//! Device definition types for foundry primitives

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::common::Identifier;
use crate::lexer::Span;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Device definition: `device NMOS:` (v0.1.6)
///
/// v0.2.0: Supports optional `export` keyword for visibility control
///
/// Defines the physical contract for a foundry primitive (transistor, diode, etc.)
/// Specifies required terminals and expected materials for each terminal.
///
/// Sprint 1.5: Enhanced to support multiple allowed materials per terminal
/// Sprint 4.1: Enhanced to support parameter tolerance specifications
/// v0.2.1: Added SPICE export metadata for netlist generation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub terminals: SmallVec<[CompactString; 4]>,
    /// Terminal materials: terminal_name -> allowed material(s)
    /// Can be single material or list of alternatives
    pub materials: FxHashMap<CompactString, SmallVec<[CompactString; 2]>>,
    /// Parameter tolerance specifications (e.g., W: 1%, L: 1%, AS: 5%)
    /// Values are relative tolerances (0.01 = 1%)
    pub tolerance: Option<FxHashMap<CompactString, f64>>,
    /// Declarative physical metric extraction rules (v0.2.1+)
    pub metrics: Option<FxHashMap<CompactString, MetricExpression>>,
    /// SPICE export metadata (v0.2.1)
    pub spice_info: Option<SpiceExportInfo>,
    pub span: Span,
}

/// Strongly-typed 2D Manifold Set Expression
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManifoldExpr {
    /// Terminal polygon reference (e.g. `D`, `G`, `S`, `B`)
    Terminal(CompactString),

    /// Boolean Intersection: A ∩ B (`A & B`)
    Intersect(Box<ManifoldExpr>, Box<ManifoldExpr>),

    /// Boolean Union: A ∪ B (`A | B`)
    Union(Box<ManifoldExpr>, Box<ManifoldExpr>),

    /// Boolean Difference: A \ B (`A - B`)
    Difference(Box<ManifoldExpr>, Box<ManifoldExpr>),

    /// 2D Convex Hull Envelope bridging two manifolds (`hull(A, B)`)
    Hull(Box<ManifoldExpr>, Box<ManifoldExpr>),
}

impl ManifoldExpr {
    /// Collect all terminal names referenced in this manifold expression
    pub fn collect_terminals<'a>(&'a self, out: &mut Vec<&'a CompactString>) {
        match self {
            Self::Terminal(name) => out.push(name),
            Self::Intersect(a, b)
            | Self::Union(a, b)
            | Self::Difference(a, b)
            | Self::Hull(a, b) => {
                a.collect_terminals(out);
                b.collect_terminals(out);
            }
        }
    }
}

/// Universal topological geometric extraction metrics (v0.2.1+)
///
/// Declares the physical measurement rules in user-space without hardcoded compiler heuristics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricExpression {
    /// Reference to another metric identifier within the device block (e.g. `SA`, `W`)
    Ref(CompactString),

    /// Extent of a manifold along carrier transport flux vector: span(manifold, along: flux(from, to))
    SpanAlongFlux {
        manifold: ManifoldExpr,
        from: CompactString,
        to: CompactString,
    },

    /// Extent of a manifold along transverse normal vector: span(manifold, along: transverse(from, to))
    SpanAlongTransverse {
        manifold: ManifoldExpr,
        from: CompactString,
        to: CompactString,
    },

    /// 2D surface area of a manifold (in m²)
    Area(ManifoldExpr),

    /// 1D boundary perimeter of a manifold (in m)
    Perimeter(ManifoldExpr),

    /// Algebraic dimensional division: Metric / Metric -> (e.g. Length / Length = Dimensionless)
    Divide(Box<MetricExpression>, Box<MetricExpression>),

    /// Maxwell resistance integral: R = ρ · (L / A) across resistive channel
    Resistance {
        from: CompactString,
        to: CompactString,
    },

    /// Electrostatic parallel-plate capacitance integral: C = ε₀ · εᵣ · (A / d)
    Capacitance {
        plate_a: CompactString,
        plate_b: CompactString,
    },
}

/// SPICE parameter formatting style
///
/// Declares how parameters should be formatted in SPICE output.
/// REQUIRED field - no defaults, user must explicitly declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpiceParameterStyle {
    /// Positional values: R1 n1 n2 1000
    /// Used by: R, C, L, V, I, E, G, H, F
    Positional,
    /// Named parameters: M1 d g s b NMOS W=1u L=0.18u
    /// Used by: M, Q, J, X (transistors and subcircuits)
    Named,
}

/// SPICE export information for device definitions
///
/// This metadata tells the netlist exporter how to format the device in SPICE.
/// ALL fields are REQUIRED - no defaults, no guessing, fail loudly if missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpiceExportInfo {
    /// SPICE device prefix (R for resistor, C for capacitor, M for MOSFET, X for subcircuit, etc.)
    pub prefix: char,
    /// Ordered list of terminal names for SPICE card
    /// Example: ["A", "B"] for resistor, ["drain", "gate", "source", "bulk"] for MOSFET
    pub terminal_order: SmallVec<[CompactString; 4]>,
    /// Parameter names that should be included in SPICE card
    /// Example: ["R"] for resistor, ["W", "L", "AS", "AD"] for MOSFET
    pub parameters: SmallVec<[CompactString; 4]>,
    /// Optional model name suffix (for MOSFETs, diodes)
    pub model_name: Option<CompactString>,
    /// Parameter formatting style - REQUIRED, no default
    /// User must explicitly declare: positional or named
    pub parameter_style: SpiceParameterStyle,
    /// Optional PDK subcircuit name (for wrapped devices like sky130_fd_pr__res_high_po)
    /// If present, overrides prefix and emits X prefix with subcircuit call
    /// Example: "sky130_fd_pr__res_high_po" → "XR1 n1 n2 nGND sky130_fd_pr__res_high_po W=1.0u L=4.0u"
    pub subcircuit: Option<CompactString>,
}

impl DeviceDefinition {
    /// Get the expected materials for a terminal
    pub fn get_terminal_materials(&self, terminal: &str) -> Option<&[CompactString]> {
        self.materials.get(terminal).map(|v| v.as_slice())
    }

    /// Check if a terminal is defined for this device
    pub fn has_terminal(&self, terminal: &str) -> bool {
        self.terminals.iter().any(|t| t.as_str() == terminal)
    }

    /// Check if a material is allowed for a terminal
    pub fn is_material_allowed(&self, terminal: &str, material: &str) -> bool {
        self.materials
            .get(terminal)
            .map(|allowed| allowed.iter().any(|m| m == material))
            .unwrap_or(false)
    }

    /// Get SPICE export information
    ///
    /// Returns None if this device doesn't have SPICE export metadata defined.
    /// Callers should error if attempting to export a device without SPICE info.
    pub fn spice_info(&self) -> Option<&SpiceExportInfo> {
        self.spice_info.as_ref()
    }

    /// Get declarative physical extraction metrics
    pub fn metrics(&self) -> Option<&FxHashMap<CompactString, MetricExpression>> {
        self.metrics.as_ref()
    }
}
