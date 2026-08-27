use crate::module_resolver::ResolverError;
use crate::symbol_table::SymbolTable;
use hwc_parser::{ImportSymbols, TopLevelItem};
use miette::NamedSource;
use std::path::Path;

impl super::ModuleResolver {
    /// Register import targets into the symbol table
    pub(super) fn register_import_targets(
        &self,
        symbols: &ImportSymbols,
        program: &hwc_parser::Program,
        file_path: &Path,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        match symbols {
            ImportSymbols::All => {
                for item in &program.items {
                    if self.is_item_exported(item) {
                        self.register_item(item, symbol_table);
                    }
                }
            }
            ImportSymbols::Named(names) => {
                for name in names {
                    let name_str = name.as_str();

                    let item = program
                        .items
                        .iter()
                        .find(|item| self.item_matches_name(item, name_str));

                    if let Some(item) = item {
                        if !self.is_item_exported(item) {
                            return Err(ResolverError::PrivateSymbolAccess {
                                symbol: name_str.to_string(),
                                path: file_path.display().to_string(),
                                span: None,
                                src: NamedSource::new("", ""),
                            });
                        }

                        self.register_item(item, symbol_table);
                    } else {
                        return Err(ResolverError::SymbolNotFound {
                            symbol: name_str.to_string(),
                            path: file_path.display().to_string(),
                            span: None,
                            src: NamedSource::new("", ""),
                        });
                    }
                }
            }
            ImportSymbols::Single(name) => {
                let name_str = name.as_str();
                let item = program
                    .items
                    .iter()
                    .find(|item| self.item_matches_name(item, name_str));

                if let Some(item) = item {
                    if !self.is_item_exported(item) {
                        return Err(ResolverError::PrivateSymbolAccess {
                            symbol: name_str.to_string(),
                            path: file_path.display().to_string(),
                            span: None,
                            src: NamedSource::new("", ""),
                        });
                    }

                    self.register_item(item, symbol_table);
                } else {
                    return Err(ResolverError::SymbolNotFound {
                        symbol: name_str.to_string(),
                        path: file_path.display().to_string(),
                        span: None,
                        src: NamedSource::new("", ""),
                    });
                }
            }
        }

        Ok(())
    }

    fn is_item_exported(&self, item: &TopLevelItem) -> bool {
        match item {
            TopLevelItem::Function(f) => f.is_exported,
            TopLevelItem::Struct(s) => s.is_exported,
            TopLevelItem::Enum(e) => e.is_exported,
            TopLevelItem::Const(c) => c.is_exported,
            TopLevelItem::Export(_) => true, // Export declarations are always exported
            TopLevelItem::Space(_) => true,
            TopLevelItem::Module(_) => true,
            TopLevelItem::Material(m) => m.is_exported,
            TopLevelItem::Profile(p) => p.is_exported,
            TopLevelItem::Device(d) => d.is_exported,
            TopLevelItem::Test(_) => true,
            TopLevelItem::Statement(_) => false,
        }
    }

    fn item_matches_name(&self, item: &TopLevelItem, name: &str) -> bool {
        match item {
            TopLevelItem::Function(f) => f.name.name.as_str() == name,
            TopLevelItem::Struct(s) => s.name.name.as_str() == name,
            TopLevelItem::Enum(e) => e.name.name.as_str() == name,
            TopLevelItem::Const(c) => c.name.name.as_str() == name,
            TopLevelItem::Export(exp) => {
                // Export declarations export symbols, check if name is in the list
                exp.symbols.iter().any(|sym| sym.as_str() == name)
            }
            TopLevelItem::Space(sp) => sp.name.name.as_str() == name,
            TopLevelItem::Module(m) => m.name.name.as_str() == name,
            TopLevelItem::Material(m) => m.name.name.as_str() == name,
            TopLevelItem::Profile(p) => p.name.name.as_str() == name,
            TopLevelItem::Device(d) => d.name.name.as_str() == name,
            TopLevelItem::Test(t) => t.name.name.as_str() == name,
            TopLevelItem::Statement(_) => false,
        }
    }

    fn register_item(&self, item: &TopLevelItem, symbol_table: &mut SymbolTable) {
        match item {
            TopLevelItem::Function(f) => symbol_table.register_import_function(f.clone()),
            TopLevelItem::Struct(s) => symbol_table.register_import_struct(s.clone()),
            TopLevelItem::Enum(e) => symbol_table.register_import_enum(e.clone()),
            TopLevelItem::Const(c) => {
                // Register constants - for now, treat them similar to variables
                // TODO: Proper constant handling in symbol table
                symbol_table.register_import_function(hwc_parser::FunctionDecl {
                    is_exported: c.is_exported,
                    name: c.name.clone(),
                    parameters: vec![],
                    return_type: c.type_annotation.clone(),
                    body: hwc_parser::Block {
                        statements: vec![],
                        span: c.span,
                    },
                    span: c.span,
                });
            }
            TopLevelItem::Export(_) => {
                // Export declarations don't register themselves, they re-export other symbols
                // The symbols they export should be resolved when needed
            }
            TopLevelItem::Space(sp) => symbol_table.register_import_space(sp.clone()),
            TopLevelItem::Module(m) => symbol_table.register_import_module(m.clone()),
            TopLevelItem::Material(m) => symbol_table.register_import_material(m.clone()),
            TopLevelItem::Profile(p) => symbol_table.register_import_profile(p.clone()),
            TopLevelItem::Device(d) => symbol_table.register_import_device(d.clone()),
            TopLevelItem::Test(t) => symbol_table.register_import_test(t.clone()),
            TopLevelItem::Statement(_) => {}
        }
    }
}

