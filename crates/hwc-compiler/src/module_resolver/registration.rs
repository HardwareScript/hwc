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
        // If this import has an alias, create a new HPM layer for the namespace
        if alias.is_some() {
            symbol_table.push_hpm_layer();
        }

        match targets {
            ImportTargets::Star => {
                // Wildcard import: register all EXPORTED definitions
                for definition in &program.definitions {
                    if self.is_exported(definition) {
                        self.register_definition(definition, symbol_table)?;
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
                        .find(|d| self.def_matches_name(d, name_str));

                    if let Some(definition) = def {
                        // Found as a local definition - check if it's exported
                        if !self.is_exported(definition) {
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

                        self.register_definition(definition, symbol_table)?;
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
    pub(super) fn is_exported(&self, definition: &Definition) -> bool {
        match definition {
            Definition::Bridge(b) => b.is_exported,
            Definition::Material(m) => m.is_exported,
            Definition::Profile(p) => p.is_exported,
            Definition::Component(c) => c.is_exported,
            Definition::Module(m) => m.is_exported,
            Definition::Logic(l) => l.is_exported,
            Definition::Enum(e) => e.is_exported,
            Definition::Struct(s) => s.is_exported,
            Definition::Mechanical(m) => m.is_exported,
            Definition::Interface(i) => i.is_exported,
            Definition::Test(t) => t.is_exported,
            Definition::SignalGroup(sg) => sg.is_exported,
            Definition::Pattern(p) => p.is_exported,
            Definition::Strategy(s) => s.is_exported,
            Definition::Unit(u) => u.is_exported,
            Definition::Device(d) => d.is_exported,
            Definition::Const(c) => c.is_exported,
            Definition::Shape(s) => s.is_exported,
            Definition::MaterialAlias(a) => a.is_exported,
            Definition::Space(s) => s.is_exported, // v0.2.1: Hierarchical Space Composition
            Definition::SpiceModel(sm) => sm.is_exported, // v0.2.1: SPICE model cards
            Definition::Subcircuit(ss) => ss.is_exported, // v0.2.2: SPICE subcircuit cards
            Definition::PolymorphicInterface(_) => true, // TODO: add is_exported field
        }
    }

    /// Check if a definition matches a name
    pub(super) fn def_matches_name(&self, definition: &Definition, name: &str) -> bool {
        match definition {
            Definition::Bridge(b) => b.name.as_str() == name,
            Definition::Material(m) => m.name.as_str() == name,
            Definition::Profile(p) => p.name.as_str() == name,
            Definition::Component(c) => c.name.as_str() == name,
            Definition::Module(m) => m.name.as_str() == name,
            Definition::Logic(l) => l.name.as_str() == name,
            Definition::Enum(e) => e.name.as_str() == name,
            Definition::Struct(s) => s.name.as_str() == name,
            Definition::Mechanical(m) => m.name.as_str() == name,
            Definition::Interface(i) => i.name.as_str() == name,
            Definition::Test(t) => t.name.as_str() == name,
            Definition::SignalGroup(sg) => sg.name.as_str() == name,
            Definition::Pattern(p) => p.name.as_str() == name,
            Definition::Strategy(s) => s.name.as_str() == name,
            Definition::Unit(u) => u.symbol.as_str() == name,
            Definition::Device(d) => d.name.as_str() == name,
            Definition::Const(c) => c.name.as_str() == name,
            Definition::Shape(s) => s.name.as_str() == name,
            Definition::MaterialAlias(a) => a.name.as_str() == name,
            Definition::PolymorphicInterface(pi) => pi.name.as_str() == name,
            Definition::Space(s) => s.name.as_str() == name, // v0.2.1: Hierarchical Space Composition
            Definition::SpiceModel(sm) => sm.name.as_str() == name, // v0.2.1: SPICE model cards
            Definition::Subcircuit(ss) => ss.name.as_str() == name, // v0.2.2: SPICE subcircuit cards
        }
    }

    /// Get a definition's name for debug output
    pub(super) fn _def_name(&self, definition: &Definition) -> String {
        match definition {
            Definition::Bridge(b) => format!("Bridge({})", b.name),
            Definition::Material(m) => format!("Material({})", m.name),
            Definition::Profile(p) => format!("Profile({})", p.name),
            Definition::Component(c) => format!("Component({})", c.name),
            Definition::Module(m) => format!("Module({})", m.name),
            Definition::Logic(l) => format!("Logic({})", l.name),
            Definition::Enum(e) => format!("Enum({})", e.name),
            Definition::Struct(s) => format!("Struct({})", s.name),
            Definition::Mechanical(m) => format!("Mechanical({})", m.name),
            Definition::Interface(i) => format!("Interface({})", i.name),
            Definition::Test(t) => format!("Test({})", t.name),
            Definition::SignalGroup(sg) => format!("SignalGroup({})", sg.name),
            Definition::Pattern(p) => format!("Pattern({})", p.name),
            Definition::Strategy(s) => format!("Strategy({})", s.name),
            Definition::Unit(u) => format!("Unit({})", u.symbol),
            Definition::Device(d) => format!("Device({})", d.name),
            Definition::Const(c) => format!("Const({})", c.name),
            Definition::Shape(s) => format!("Shape({})", s.name),
            Definition::MaterialAlias(a) => format!("MaterialAlias({})", a.name),
            Definition::PolymorphicInterface(pi) => format!("PolymorphicInterface({})", pi.name),
            Definition::Space(_) => "Space".to_string(),
            Definition::SpiceModel(sm) => format!("SpiceModel({})", sm.name),
            Definition::Subcircuit(ss) => format!("Subcircuit({})", ss.name),
        }
    }
}
