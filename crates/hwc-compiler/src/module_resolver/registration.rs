//! Symbol registration logic for resolved imports (per-import, deterministic).

use crate::module_resolver::ResolverError;
use crate::symbol_table::SymbolTable;
use hwc_parser::{Definition, ImportTargets};
use miette::{NamedSource, SourceSpan};
use std::path::Path;

impl super::ModuleResolver {
    /// Register import targets into the symbol table
    ///
    /// Always executes, even for previously-loaded files (no state pollution)
    pub(super) fn register_import_targets(
        &self,
        targets: &ImportTargets,
        program: &hwc_parser::Program,
        file_path: &Path,
        alias: Option<&hwc_parser::Identifier>,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        let arena = &program.arena;

        // If this import has an alias, create a new HPM layer for the namespace
        if alias.is_some() {
            symbol_table.push_hpm_layer();
        }

        match targets {
            ImportTargets::Star => {
                // Wildcard import: register all EXPORTED definitions
                for definition in &program.definitions {
                    if self.is_exported(definition, arena) {
                        self.register_definition(definition, arena, symbol_table)?;
                    }
                }
            }
            ImportTargets::List(names) => {
                // Selective import: register only requested symbols
                for name in names {
                    let name_str = name.as_str();

                    // Find the definition in this module's definitions
                    let def = program
                        .definitions
                        .iter()
                        .find(|d| self.def_matches_name(d, name_str, arena));

                    if let Some(definition) = def {
                        // Found as a local definition - check if it's exported
                        if !self.is_exported(definition, arena) {
                            return Err(ResolverError::PrivateSymbolAccess {
                                symbol: name_str.to_string(),
                                path: file_path.display().to_string(),
                                span: Some(SourceSpan::new(
                                    name.span.start.into(),
                                    (name.span.end - name.span.start).into(),
                                )),
                                src: NamedSource::new("", ""),
                            });
                        }

                        self.register_definition(definition, arena, symbol_table)?;
                    } else {
                        // Not found in definitions - check if it's re-exported
                        let is_reexported = program
                            .re_exports
                            .iter()
                            .any(|re| re.symbol.as_str() == name_str);

                        if is_reexported {
                            // This symbol was imported by this module and re-exported.
                            // It should already be in the symbol table from recursive resolution.
                        } else {
                            return Err(ResolverError::SymbolNotFound {
                                symbol: name_str.to_string(),
                                path: file_path.display().to_string(),
                                span: Some(SourceSpan::new(
                                    name.span.start.into(),
                                    (name.span.end - name.span.start).into(),
                                )),
                                src: NamedSource::new("", ""),
                            });
                        }
                    }
                }
            }
        }

        // Register the namespace alias if present
        if let Some(alias_ident) = alias {
            symbol_table.register_namespace_alias(alias_ident.as_str().to_string().into());
        }

        Ok(())
    }

    /// Check if a definition is exported
    pub(super) fn is_exported(
        &self,
        definition: &Definition,
        arena: &hwc_parser::ast::arena::AstArena,
    ) -> bool {
        match definition {
            Definition::Bridge(id) => arena.bridge_defs[*id].is_exported,
            Definition::Material(id) => arena.material_defs[*id].is_exported,
            Definition::Profile(id) => arena.profile_defs[*id].is_exported,
            Definition::Component(id) => arena.component_defs[*id].is_exported,
            Definition::Module(id) => arena.module_defs[*id].is_exported,
            Definition::Logic(l) => arena.logic_defs[*l].is_exported,
            Definition::Enum(e) => arena.enum_defs[*e].is_exported,
            Definition::Struct(s) => arena.struct_defs[*s].is_exported,
            Definition::Mechanical(id) => arena.mechanical_defs[*id].is_exported,
            Definition::Interface(id) => arena.interface_defs[*id].is_exported,
            Definition::Test(id) => arena.test_defs[*id].is_exported,
            Definition::SignalGroup(sg) => arena.signal_group_defs[*sg].is_exported,
            Definition::Pattern(p) => arena.pattern_defs[*p].is_exported,
            Definition::Strategy(s) => arena.strategy_defs[*s].is_exported,
            Definition::Unit(id) => arena.unit_defs[*id].is_exported,
            Definition::Device(id) => arena.device_defs[*id].is_exported,
            Definition::Const(id) => arena.const_defs[*id].is_exported,
            Definition::Shape(s) => arena.shape_defs[*s].is_exported,
            Definition::MaterialAlias(a) => arena.material_alias_defs[*a].is_exported,
            Definition::Space(id) => arena.space_defs[*id].is_exported, // v0.2.1: Hierarchical Space Composition
            Definition::SpiceModel(sm) => arena.spice_model_defs[*sm].is_exported, // v0.2.1: SPICE model cards
            Definition::Subcircuit(ss) => arena.subcircuit_defs[*ss].is_exported, // v0.2.2: SPICE subcircuit cards
            Definition::PolymorphicInterface(_) => true, // TODO: add is_exported field
        }
    }

    /// Check if a definition matches a name
    pub(super) fn def_matches_name(
        &self,
        definition: &Definition,
        name: &str,
        arena: &hwc_parser::ast::arena::AstArena,
    ) -> bool {
        match definition {
            Definition::Bridge(id) => arena.bridge_defs[*id].name.as_str() == name,
            Definition::Material(id) => arena.material_defs[*id].name.as_str() == name,
            Definition::Profile(id) => arena.profile_defs[*id].name.as_str() == name,
            Definition::Component(c) => arena.component_defs[*c].name.as_str() == name,
            Definition::Module(id) => arena.module_defs[*id].name.as_str() == name,
            Definition::Logic(l) => arena.logic_defs[*l].name.as_str() == name,
            Definition::Enum(e) => arena.enum_defs[*e].name.as_str() == name,
            Definition::Struct(s) => arena.struct_defs[*s].name.as_str() == name,
            Definition::Mechanical(id) => arena.mechanical_defs[*id].name.as_str() == name,
            Definition::Interface(id) => arena.interface_defs[*id].name.as_str() == name,
            Definition::Test(id) => arena.test_defs[*id].name.as_str() == name,
            Definition::SignalGroup(sg) => arena.signal_group_defs[*sg].name.as_str() == name,
            Definition::Pattern(p) => arena.pattern_defs[*p].name.as_str() == name,
            Definition::Strategy(s) => arena.strategy_defs[*s].name.as_str() == name,
            Definition::Unit(id) => arena.unit_defs[*id].symbol.as_str() == name,
            Definition::Device(id) => arena.device_defs[*id].name.as_str() == name,
            Definition::Const(id) => arena.const_defs[*id].name.as_str() == name,
            Definition::Shape(s) => arena.shape_defs[*s].name.as_str() == name,
            Definition::MaterialAlias(a) => arena.material_alias_defs[*a].name.as_str() == name,
            Definition::PolymorphicInterface(id) => arena.interface_defs[*id].name.as_str() == name,
            Definition::Space(id) => arena.space_defs[*id].name.as_str() == name, // v0.2.1: Hierarchical Space Composition
            Definition::SpiceModel(sm) => arena.spice_model_defs[*sm].name.as_str() == name, // v0.2.1: SPICE model cards
            Definition::Subcircuit(ss) => arena.subcircuit_defs[*ss].name.as_str() == name, // v0.2.2: SPICE subcircuit cards
        }
    }

    /// Get a definition's name for debug output
    pub(super) fn _def_name(
        &self,
        definition: &Definition,
        arena: &hwc_parser::ast::arena::AstArena,
    ) -> String {
        match definition {
            Definition::Bridge(id) => format!("Bridge({})", arena.bridge_defs[*id].name),
            Definition::Material(id) => format!("Material({})", arena.material_defs[*id].name),
            Definition::Profile(id) => format!("Profile({})", arena.profile_defs[*id].name),
            Definition::Component(c) => format!("Component({})", arena.component_defs[*c].name),
            Definition::Module(id) => format!("Module({})", arena.module_defs[*id].name),
            Definition::Logic(l) => format!("Logic({})", arena.logic_defs[*l].name),
            Definition::Enum(e) => format!("Enum({})", arena.enum_defs[*e].name),
            Definition::Struct(s) => format!("Struct({})", arena.struct_defs[*s].name),
            Definition::Mechanical(id) => {
                format!("Mechanical({})", arena.mechanical_defs[*id].name)
            }
            Definition::Interface(id) => format!("Interface({})", arena.interface_defs[*id].name),
            Definition::Test(id) => format!("Test({})", arena.test_defs[*id].name),
            Definition::SignalGroup(sg) => {
                format!("SignalGroup({})", arena.signal_group_defs[*sg].name)
            }
            Definition::Pattern(p) => format!("Pattern({})", arena.pattern_defs[*p].name),
            Definition::Strategy(s) => format!("Strategy({})", arena.strategy_defs[*s].name),
            Definition::Unit(id) => format!("Unit({})", arena.unit_defs[*id].symbol),
            Definition::Device(id) => format!("Device({})", arena.device_defs[*id].name),
            Definition::Const(id) => format!("Const({})", arena.const_defs[*id].name),
            Definition::Shape(s) => format!("Shape({})", arena.shape_defs[*s].name),
            Definition::MaterialAlias(a) => {
                format!("MaterialAlias({})", arena.material_alias_defs[*a].name)
            }
            Definition::PolymorphicInterface(id) => {
                format!("PolymorphicInterface({})", arena.interface_defs[*id].name)
            }
            Definition::Space(_) => "Space".to_string(),
            Definition::SpiceModel(sm) => {
                format!("SpiceModel({})", arena.spice_model_defs[*sm].name)
            }
            Definition::Subcircuit(ss) => {
                format!("Subcircuit({})", arena.subcircuit_defs[*ss].name)
            }
        }
    }
}
