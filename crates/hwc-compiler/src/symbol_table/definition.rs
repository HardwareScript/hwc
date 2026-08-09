//! Arena-based Definition References for Symbol Table
//!
//! v0.2.1 Arena Refactor: The compiler now uses the parser's arena-based Definition enum directly.
//! No separate enum, no struct copying - just 4-byte IDs pointing into the shared arena.
//!
//! This provides:
//! - Zero memory duplication
//! - Blazing fast lookups (4-byte ID copy vs full struct clone)
//! - Single source of truth (parser's AstArena)

use hwc_parser::ast::AstArena;

// Re-export the parser's arena-based Definition enum
pub use hwc_parser::ast::Definition;

/// Arena-aware extension methods for `Definition`.
///
/// `Definition` is defined in the parser crate, so we cannot implement an
/// inherent `impl` for it here. Instead we expose these helpers via a trait
/// that must be imported (`use crate::symbol_table::definition::DefinitionExt;`)
/// wherever `.kind_str()` or `.is_exported(..)` are needed.
pub trait DefinitionExt {
    /// Human-readable type name for diagnostics (e.g. "material", "component")
    fn kind_str(&self) -> &'static str;

    /// Check if the definition is marked as exported
    /// Takes arena reference to look up actual definition data
    fn is_exported(&self, arena: &AstArena) -> bool;
}

impl DefinitionExt for Definition {
    /// Human-readable type name for diagnostics (e.g. "material", "component")
    fn kind_str(&self) -> &'static str {
        match self {
            Definition::Material(_) => "material",
            Definition::MaterialAlias(_) => "material alias",
            Definition::Profile(_) => "profile",
            Definition::Component(_) => "component",
            Definition::Module(_) => "module",
            Definition::Mechanical(_) => "mechanical",
            Definition::Interface(_) | Definition::PolymorphicInterface(_) => "interface",
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
    /// Takes arena reference to look up actual definition data
    fn is_exported(&self, arena: &AstArena) -> bool {
        match self {
            Definition::Material(id) => arena.material_defs[*id].is_exported,
            Definition::Profile(id) => arena.profile_defs[*id].is_exported,
            Definition::Component(id) => arena.component_defs[*id].is_exported,
            Definition::Module(id) => arena.module_defs[*id].is_exported,
            Definition::Mechanical(id) => arena.mechanical_defs[*id].is_exported,
            Definition::Interface(id) | Definition::PolymorphicInterface(id) => {
                arena.interface_defs[*id].is_exported
            }
            Definition::Test(id) => arena.test_defs[*id].is_exported,
            Definition::SignalGroup(id) => arena.signal_group_defs[*id].is_exported,
            Definition::Pattern(id) => arena.pattern_defs[*id].is_exported,
            Definition::Strategy(id) => arena.strategy_defs[*id].is_exported,
            Definition::Logic(id) => arena.logic_defs[*id].is_exported,
            Definition::Enum(id) => arena.enum_defs[*id].is_exported,
            Definition::Struct(id) => arena.struct_defs[*id].is_exported,
            Definition::Device(id) => arena.device_defs[*id].is_exported,
            Definition::Shape(id) => arena.shape_defs[*id].is_exported,
            Definition::Space(id) => arena.space_defs[*id].is_exported,
            Definition::SpiceModel(id) => arena.spice_model_defs[*id].is_exported,
            Definition::Subcircuit(id) => arena.subcircuit_defs[*id].is_exported,
            // Bridges and other types without export flags - implicitly public
            _ => true,
        }
    }
}
