//! Module definition types for v0.1.4
//!
//! Modules represent purely logical/electrical connections with NO physical coordinates.
//! They support comptime evaluation (for loops, if conditionals) and can be nested.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::common::Identifier;
use super::component::Parameter;
use crate::lexer::Span;
use compact_str::CompactString;

/// Module definition: `module Name:` (v0.1.6 + v0.1.7 Physical Macros)
///
/// Modules now support intrinsic physical layout (relative only) for Physical Macros.
/// This cures the "Physical Pile" where logical modules instantiated sub-components at [0,0,0].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub name: Identifier,
    pub pins: Vec<PinDeclaration>,
    pub statements: Vec<ModuleStatement>,
    pub logic: Option<super::logic::LogicBlock>, // NEW: Logic synthesis support (v0.3.0)
    /// v0.1.7 Physical Macros (Physical Pile Paradox fix): intrinsic layout using relative coords only.
    /// When present, sub-component positions are defined here (relative to module origin) so modules
    /// carry physical structure and do not pile at [0,0,0] on instantiation.
    pub intrinsic_layout: Option<Vec<super::space::LayoutStatement>>,
    pub span: Span,
}

/// Pin direction for electrical borrow checking
///
/// Enables the compiler to prevent short circuits and validate connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinDirection {
    Input,
    Output,
    Inout,
    Power,
    Ground,
    Passive, // Default - no specific direction
}

impl std::fmt::Display for PinDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinDirection::Input => write!(f, "input"),
            PinDirection::Output => write!(f, "output"),
            PinDirection::Inout => write!(f, "inout"),
            PinDirection::Power => write!(f, "power"),
            PinDirection::Ground => write!(f, "ground"),
            PinDirection::Passive => write!(f, "passive"),
        }
    }
}

/// Pin declaration in module
///
/// Supports:
/// - Simple pins: `input VIN`, `output VOUT`
/// - Array pins (buses): `input Bus_A[64]`, `output DataBus[32]`
/// - Directionless pins: `VCC` (defaults to Passive)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinDeclaration {
    pub name: CompactString,
    pub direction: PinDirection,
    pub array_size: Option<usize>, // Some(64) for Bus[64], None for simple pins
    pub span: Span,
}

/// Statements inside a module definition
///
/// Modules can contain:
/// - Component instantiation (add)
/// - Routing (route)
/// - Comptime for loops
/// - Comptime if conditionals
/// - Nested module instantiation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModuleStatement {
    /// Add component: `add ComponentType (params) named Instance`
    /// Note: NO `at [x,y,z]` allowed in modules (that's in space layout blocks)
    AddComponent(ModuleComponentPlacement),

    /// Route: `route From.Pin to To.Pin`
    /// Note: NO waypoints allowed in modules (pure logical connection)
    Route(ModuleRoute),

    /// Comptime for loop: `for i in 0..63:`
    For(ForLoop),

    /// Comptime if conditional: `if condition:`
    If(IfConditional),
}

/// Component placement inside a module (NO coordinates)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleComponentPlacement {
    pub component_type: CompactString,
    pub parameters: SmallVec<[Parameter; 4]>,
    pub name: Option<CompactString>,
    pub array_index: Option<ArrayIndex>, // For `named Bit[i]` in for loops
    pub span: Span,
}

/// Route inside a module (NO waypoints, pure logical connection)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleRoute {
    pub from: ModulePinReference,
    pub to: ModulePinReference,
    pub span: Span,
}

/// Pin reference inside a module (supports array indexing)
///
/// # Parser Convention
/// When there's no dot in the reference, `component` is empty and `pin` contains the net name:
/// - `M1.drain` → component="M1", pin="drain" (device terminal.into())
/// - `VOUT`     → component="", pin="VOUT" (net name.into())
///
/// Examples:
/// - `Component.Pin` - simple pin reference
/// - `Component[i].Pin` - array component with loop variable
/// - `Bus[i]` - array pin with loop variable (component="", pin="Bus")
/// - `Bit[i-1].CarryOut` - arithmetic in array index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModulePinReference {
    pub component: CompactString,
    pub component_index: Option<ArrayIndex>, // For Component[i]
    pub pin: CompactString,
    pub pin_index: Option<ArrayIndex>, // For Bus[i]
    pub span: Span,
}

/// Array index expression (used in for loops)
///
/// Examples:
/// - `i` - simple variable
/// - `i-1` - arithmetic expression
/// - `0` - literal constant
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrayIndex {
    /// Variable reference: `i`
    Variable(String),
    /// Literal constant: `0`, `63`
    Literal(usize),
    /// Arithmetic: `i-1`, `i+1`
    Arithmetic {
        left: Box<ArrayIndex>,
        op: ArithmeticOp,
        right: Box<ArrayIndex>,
    },
}

/// Arithmetic operators for array indexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithmeticOp {
    Add,      // +
    Subtract, // -
    Multiply, // *
    Divide,   // /
}

/// Comptime for loop: `for i in 0..63:`
///
/// Evaluated at compile time - generates multiple statements
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForLoop {
    pub variable: CompactString,    // Loop variable name (e.g., "i")
    pub start: usize,               // Start value (inclusive)
    pub end: usize,                 // End value (inclusive, Ruby-style)
    pub body: Vec<ModuleStatement>, // Statements inside loop
    pub span: Span,
}

/// Comptime if conditional: `if condition:`
///
/// Evaluated at compile time - only one branch is kept
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfConditional {
    pub condition: Condition,
    pub then_body: Vec<ModuleStatement>,
    pub else_body: Option<Vec<ModuleStatement>>, // Optional else block
    pub span: Span,
}

/// Condition for if statements
///
/// Examples:
/// - `i == 0` - equality check
/// - `i < 63` - less than
/// - `i > 0` - greater than
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Equality: `i == 0`
    Equals { left: ArrayIndex, right: ArrayIndex },
    /// Less than: `i < 63`
    LessThan { left: ArrayIndex, right: ArrayIndex },
    /// Greater than: `i > 0`
    GreaterThan { left: ArrayIndex, right: ArrayIndex },
    /// Not equals: `i != 0`
    NotEquals { left: ArrayIndex, right: ArrayIndex },
}

// REMOVED (pre-release cleanup): LayoutMapping and LayoutMappingStatement (old module layout syntax).
// Replaced by: ModuleLayoutBlock + LayoutStatement in space.rs (integrated with space statements for ordering).
// Why removed: Zero external/internal uses (grep confirmed). Old design separated layouts from spaces.
// Mistake to avoid: Don't keep deprecated parallel type hierarchies for features that get relocated during redesign;
// delete unused legacy types immediately in 0.x to prevent confusion and accidental use.
