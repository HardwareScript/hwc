//! Abstract Syntax Tree for Hardware Script
//!
//! Based on v0.1.3 specification with indentation-based syntax.
//! See `grammar/hardware.grammar` for complete syntax rules.

use serde::{Deserialize, Serialize};

pub mod arena;
mod bridge;
mod common;
mod component;
mod const_def;
pub mod device;
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
mod spice_model;
mod subcircuit;
mod test;
mod unit;

// Re-export all public types
pub use arena::AstArena;
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
pub use spice_model::*;
pub use subcircuit::*;
pub use test::*;
pub use unit::*;

// Re-export Span from lexer for use in AST
pub use crate::lexer::Span;

/// Root AST node representing a complete Hardware Script file (v0.1.4)
///
/// v0.2.0: Adds re_exports for explicit symbol re-exporting (Rust-style pub use)
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub imports: Vec<Import>,
    pub re_exports: Vec<ReExport>,
    pub definitions: Vec<Definition>,
    pub arena: arena::AstArena,
    pub span: Span,
}

/// Top-level definition (v0.1.4 unified syntax)
/// v0.2.0: Adds Bridge as first-class definition
/// v0.2.1: 100% Arena-based - ALL variants hold 4-byte IDs (zero exceptions!)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Definition {
    Bridge(arena::BridgeDefId),
    Material(arena::MaterialDefId),
    Profile(arena::ProfileDefId),
    Component(arena::ComponentDefId),
    Module(arena::ModuleDefId),
    Mechanical(arena::MechanicalDefId),
    Interface(arena::InterfaceDefId),
    PolymorphicInterface(arena::InterfaceDefId), // Reuse InterfaceDefId
    Test(arena::TestDefId),
    Space(arena::SpaceDefId),
    Unit(arena::UnitDefId),
    Device(arena::DeviceDefId),
    Const(arena::ConstDefId),
    // 100% uniform - all types are now 4-byte IDs!
    SignalGroup(arena::SignalGroupDefId),
    Pattern(arena::PatternDefId),
    Strategy(arena::StrategyDefId),
    MaterialAlias(arena::MaterialAliasDefId),
    Enum(arena::EnumDefId),
    Struct(arena::StructDefId),
    Logic(arena::LogicDefId),
    Shape(arena::ShapeDefId),
    SpiceModel(arena::SpiceModelDefId),
    Subcircuit(arena::SubcircuitDefId),
}

// REMOVED (pre-release cleanup): Legacy AST struct (the old non-Program wrapper).
// Why it existed: Early design before unified Program/Definition enum (v0.1.4).
// Why removed: No longer referenced anywhere (grep confirmed zero uses). Pre-1.0, dead code for compat is forbidden.
// Pattern avoided: Don't leave #[deprecated] stubs "just in case"; delete when unused. Future: if reintroducing old API,
// use versioned crates or feature flags, not deprecated items in main tree.
