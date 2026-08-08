//! Unified Definition Enum for Symbol Table
//!
//! This module provides a single enum that wraps all top-level AST definitions,
//! eliminating the need for 20+ separate FxHashMap fields in SymbolLayer.

use hwc_parser::logic::{EnumDefinition, LogicDefinition, StructDefinition};
use hwc_parser::{
    BridgeDefinition, ComponentDefinition, ConstDefinition, DeviceDefinition, InterfaceDefinition,
    MaterialAliasDefinition, MaterialDefinition, MechanicalDefinition, ModuleDefinition,
    PatternDefinition, ProfileDefinition, ShapeDefinition, SignalGroupDefinition, SpaceDefinition,
    SpiceModelDefinition, StrategyDefinition, SubcircuitDefinition, TestDefinition, UnitDefinition,
};

/// Unified enum representing any top-level declaration in HardwareScript
///
/// This eliminates struct field explosion in SymbolLayer - instead of 20 separate
/// FxHashMap fields, we have ONE map: FxHashMap<CompactString, Definition>
#[derive(Debug, Clone)]
pub enum Definition<'ast> {
    Material(MaterialDefinition),
    MaterialAlias(MaterialAliasDefinition),
    Profile(ProfileDefinition),
    Component(ComponentDefinition),
    Module(ModuleDefinition<'ast>),
    Mechanical(MechanicalDefinition),
    Interface(InterfaceDefinition),
    Test(TestDefinition),
    SignalGroup(SignalGroupDefinition),
    Pattern(PatternDefinition),
    Strategy(StrategyDefinition),
    Logic(LogicDefinition),
    Enum(EnumDefinition),
    Struct(StructDefinition),
    Unit(UnitDefinition),
    Device(DeviceDefinition),
    Const(ConstDefinition),
    Shape(ShapeDefinition),
    Space(SpaceDefinition<'ast>),
    Bridge(BridgeDefinition),
    SpiceModel(SpiceModelDefinition), // v0.2.1: SPICE model card definitions
    Subcircuit(SubcircuitDefinition), // v0.3.0: Native typed subcircuit definitions (replaces raw SPICE strings)
}

impl<'ast> Definition<'ast> {
    /// Human-readable type name for diagnostics (e.g. "material", "component")
    pub fn kind_str(&self) -> &'static str {
        match self {
            Definition::Material(_) => "material",
            Definition::MaterialAlias(_) => "material alias",
            Definition::Profile(_) => "profile",
            Definition::Component(_) => "component",
            Definition::Module(_) => "module",
            Definition::Mechanical(_) => "mechanical",
            Definition::Interface(_) => "interface",
            Definition::Test(_) => "test",
            Definition::SignalGroup(_) => "signal_group",
            Definition::Pattern(_) => "pattern",
            Definition::Strategy(_) => "strategy",
            Definition::Logic(_) => "logic",
            Definition::Enum(_) => "enum",
            Definition::Struct(_) => "struct",
            Definition::Unit(_) => "unit",
            Definition::Device(_) => "device",
            Definition::Const(_) => "constant",
            Definition::Shape(_) => "shape",
            Definition::Space(_) => "space",
            Definition::Bridge(_) => "bridge",
            Definition::SpiceModel(_) => "spice_model",
            Definition::Subcircuit(_) => "subcircuit",
        }
    }

    /// Check if the definition is marked as exported
    pub fn is_exported(&self) -> bool {
        match self {
            Definition::Material(d) => d.is_exported,
            Definition::Profile(d) => d.is_exported,
            Definition::Component(d) => d.is_exported,
            Definition::Module(d) => d.is_exported,
            Definition::Mechanical(d) => d.is_exported,
            Definition::Interface(d) => d.is_exported,
            Definition::Test(d) => d.is_exported,
            Definition::SignalGroup(d) => d.is_exported,
            Definition::Pattern(d) => d.is_exported,
            Definition::Strategy(d) => d.is_exported,
            Definition::Logic(d) => d.is_exported,
            Definition::Enum(d) => d.is_exported,
            Definition::Struct(d) => d.is_exported,
            Definition::Device(d) => d.is_exported,
            Definition::Shape(d) => d.is_exported,
            Definition::Space(d) => d.is_exported,
            Definition::SpiceModel(d) => d.is_exported,
            Definition::Subcircuit(d) => d.is_exported,
            // Local/prelude items without export flags are implicitly public
            _ => true,
        }
    }

    /// Get the name of the definition (for diagnostics)
    pub fn name(&self) -> &str {
        match self {
            Definition::Material(d) => d.name.as_str(),
            Definition::MaterialAlias(d) => d.name.as_str(),
            Definition::Profile(d) => d.name.as_str(),
            Definition::Component(d) => d.name.as_str(),
            Definition::Module(d) => d.name.as_str(),
            Definition::Mechanical(d) => d.name.as_str(),
            Definition::Interface(d) => d.name.as_str(),
            Definition::Test(d) => d.name.as_str(),
            Definition::SignalGroup(d) => d.name.as_str(),
            Definition::Pattern(d) => d.name.as_str(),
            Definition::Strategy(d) => d.name.as_str(),
            Definition::Logic(d) => d.name.as_str(),
            Definition::Enum(d) => d.name.as_str(),
            Definition::Struct(d) => d.name.as_str(),
            Definition::Unit(d) => d.name.as_str(),
            Definition::Device(d) => d.name.as_str(),
            Definition::Const(d) => d.name.as_str(),
            Definition::Shape(d) => d.name.as_str(),
            Definition::Space(d) => d.name.as_str(),
            Definition::SpiceModel(d) => d.name.as_str(),
            Definition::Subcircuit(d) => d.name.as_str(),
            Definition::Bridge(_) => "<bridge>", // Bridges don't have names
        }
    }
}
