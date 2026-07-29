//! Abstract Syntax Tree for Hardware Script
//!
//! Based on v0.1.3 specification with indentation-based syntax.
//! See `grammar/hardware.grammar` for complete syntax rules.

mod bridge;
mod common;
mod component;
mod const_def;
mod device;
mod device_contract;
pub mod expr;
mod expression;
mod import;
mod interface;
pub mod logic;
mod material;
mod mechanical;
mod module;
mod pattern;
mod polymorphic_interface;
pub mod profile;
mod shape;
mod signal_group;
mod space;
mod test;
mod unit;

// Re-export all public types
pub use bridge::*;
pub use common::*;
pub use component::*;
pub use const_def::*;
pub use device::*;
pub use device_contract::*;
pub use expression::*;
pub use import::*;
pub use interface::*;
pub use logic::*;
pub use material::*;
pub use mechanical::*;
pub use module::*;
pub use pattern::*;
pub use polymorphic_interface::*;
pub use profile::*;
pub use shape::*;
pub use signal_group::*;
pub use space::*;
pub use test::*;
pub use unit::*;

// Re-export Span from lexer for use in AST
pub use crate::lexer::Span;

use serde::{Deserialize, Serialize};

/// Root AST node representing a complete Hardware Script file (v0.1.4)
///
/// v0.2.0: Adds re_exports for explicit symbol re-exporting (Rust-style pub use)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub imports: Vec<Import>,
    pub re_exports: Vec<ReExport>,
    pub definitions: Vec<Definition>,
    pub span: Span,
}

/// Top-level definition (v0.1.4 unified syntax)
/// v0.2.0: Adds Bridge as first-class definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Definition {
    Bridge(BridgeDefinition), // NEW v0.2.0: First-class bridge definitions
    Material(MaterialDefinition),
    Profile(Box<ProfileDefinition>),
    Component(ComponentDefinition),
    Module(ModuleDefinition), // NEW: Module support for v0.1.4
    Mechanical(MechanicalDefinition),
    Interface(InterfaceDefinition),
    PolymorphicInterface(PolymorphicInterfaceDefinition), // NEW: Duck-typed interfaces for v0.1.5
    Test(TestDefinition),
    Space(SpaceDefinition),
    Unit(UnitDefinition),                   // Standard library unit definitions
    Device(DeviceDefinition), // NEW: Device definitions for v0.1.6 (foundry primitives)
    Const(ConstDefinition),   // NEW: Constant definitions for v0.1.6 (math.hw primitives)
    SignalGroup(SignalGroupDefinition), // Signal grouping for impedance control
    Pattern(PatternDefinition), // Routing pattern definitions
    Strategy(StrategyDefinition), // Routing strategy definitions
    MaterialAlias(MaterialAliasDefinition), // NEW: Material alias for stdlib
    Enum(logic::EnumDefinition), // NEW: Enum definitions for v0.4.0
    Struct(logic::StructDefinition), // NEW: Struct definitions for v0.4.0
    Logic(logic::LogicDefinition), // NEW: Logic block definitions for v0.4.0
    Shape(ShapeDefinition),   // NEW: Custom 2D polygon shape definitions
}

// REMOVED (pre-release cleanup): Legacy AST struct (the old non-Program wrapper).
// Why it existed: Early design before unified Program/Definition enum (v0.1.4).
// Why removed: No longer referenced anywhere (grep confirmed zero uses). Pre-1.0, dead code for compat is forbidden.
// Pattern avoided: Don't leave #[deprecated] stubs "just in case"; delete when unused. Future: if reintroducing old API,
// use versioned crates or feature flags, not deprecated items in main tree.
