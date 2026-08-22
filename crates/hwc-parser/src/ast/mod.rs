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

impl Program {
    /// Rebase all definition IDs in this program using the provided arena offsets.
    pub fn rebase_arena_ids(&mut self, offsets: &arena::AstArenaOffsets) {
        use arena::Idx;
        for def in &mut self.definitions {
            match def {
                Definition::Bridge(id) => *id = arena::BridgeDefId::new(id.index() + offsets.bridge_defs),
                Definition::Material(id) => *id = arena::MaterialDefId::new(id.index() + offsets.material_defs),
                Definition::Profile(id) => *id = arena::ProfileDefId::new(id.index() + offsets.profile_defs),
                Definition::Component(id) => *id = arena::ComponentDefId::new(id.index() + offsets.component_defs),
                Definition::Module(id) => *id = arena::ModuleDefId::new(id.index() + offsets.module_defs),
                Definition::Mechanical(id) => *id = arena::MechanicalDefId::new(id.index() + offsets.mechanical_defs),
                Definition::Interface(id) => *id = arena::InterfaceDefId::new(id.index() + offsets.interface_defs),
                Definition::PolymorphicInterface(id) => *id = arena::InterfaceDefId::new(id.index() + offsets.polymorphic_interface_defs),
                Definition::Test(id) => *id = arena::TestDefId::new(id.index() + offsets.test_defs),
                Definition::Space(id) => *id = arena::SpaceDefId::new(id.index() + offsets.space_defs),
                Definition::Unit(id) => *id = arena::UnitDefId::new(id.index() + offsets.unit_defs),
                Definition::Device(id) => *id = arena::DeviceDefId::new(id.index() + offsets.device_defs),
                Definition::Const(id) => *id = arena::ConstDefId::new(id.index() + offsets.const_defs),
                Definition::SignalGroup(id) => *id = arena::SignalGroupDefId::new(id.index() + offsets.signal_group_defs),
                Definition::Pattern(id) => *id = arena::PatternDefId::new(id.index() + offsets.pattern_defs),
                Definition::Strategy(id) => *id = arena::StrategyDefId::new(id.index() + offsets.strategy_defs),
                Definition::MaterialAlias(id) => *id = arena::MaterialAliasDefId::new(id.index() + offsets.material_alias_defs),
                Definition::Enum(id) => *id = arena::EnumDefId::new(id.index() + offsets.enum_defs),
                Definition::Struct(id) => *id = arena::StructDefId::new(id.index() + offsets.struct_defs),
                Definition::Logic(id) => *id = arena::LogicDefId::new(id.index() + offsets.logic_defs),
                Definition::Shape(id) => *id = arena::ShapeDefId::new(id.index() + offsets.shape_defs),
                Definition::SpiceModel(id) => *id = arena::SpiceModelDefId::new(id.index() + offsets.spice_model_defs),
                Definition::Subcircuit(id) => *id = arena::SubcircuitDefId::new(id.index() + offsets.subcircuit_defs),
            }
        }
    }
}
