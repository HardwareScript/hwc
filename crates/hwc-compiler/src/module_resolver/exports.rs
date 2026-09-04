//! Strongly-typed Module Exports interface (v0.3.1 Clean Architecture)
//!
//! Replaces ad-hoc string matching and linear AST scans with a dedicated
//! `ModuleExports` table that represents everything a module exposes.

use compact_str::CompactString;
use hwc_parser::{
    ConstDecl, DeviceDecl, EnumDecl, FunctionDecl, MaterialDecl, ModuleDecl, ProfileDecl, Program,
    SpaceDecl, StructDecl, TestDecl, TopLevelItem,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::symbol_table::{Definition, SymbolTable};

/// Represents a single exported symbol with its typed declaration or definition reference
#[derive(Debug, Clone)]
pub enum ExportedItem {
    Function(FunctionDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Const(ConstDecl),
    Space(SpaceDecl),
    Module(ModuleDecl),
    Material(MaterialDecl),
    Profile(ProfileDecl),
    Device(DeviceDecl),
    Test(TestDecl),
    ReExport(Definition),
}

/// The evaluated public export interface of a module
#[derive(Debug, Clone, Default)]
pub struct ModuleExports {
    /// Publicly exported items by symbol name
    pub items: FxHashMap<CompactString, ExportedItem>,
    /// Set of private symbol names defined in the module (for precise error reporting)
    pub private_symbols: FxHashSet<CompactString>,
}

impl ModuleExports {
    /// Analyze a parsed program and build its public export table
    pub fn from_program(program: &Program, symbol_table: &SymbolTable) -> Self {
        let mut exports = ModuleExports::default();
        let mut local_decls: FxHashMap<CompactString, ExportedItem> = FxHashMap::default();

        // 1. Index all local declarations in the module
        for item in &program.items {
            match item {
                TopLevelItem::Function(f) => {
                    let name = f.name.name.clone();
                    let exported_item = ExportedItem::Function(f.clone());
                    if f.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Struct(s) => {
                    let name = s.name.name.clone();
                    let exported_item = ExportedItem::Struct(s.clone());
                    if s.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Enum(e) => {
                    let name = e.name.name.clone();
                    let exported_item = ExportedItem::Enum(e.clone());
                    if e.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Const(c) => {
                    let name = c.name.name.clone();
                    let exported_item = ExportedItem::Const(c.clone());
                    if c.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Space(sp) => {
                    let name = sp.name.name.clone();
                    let exported_item = ExportedItem::Space(sp.clone());
                    exports.items.insert(name.clone(), exported_item.clone());
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Module(m) => {
                    let name = m.name.name.clone();
                    let exported_item = ExportedItem::Module(m.clone());
                    exports.items.insert(name.clone(), exported_item.clone());
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Material(m) => {
                    let name = m.name.name.clone();
                    let exported_item = ExportedItem::Material(m.clone());
                    if m.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Profile(p) => {
                    let name = p.name.name.clone();
                    let exported_item = ExportedItem::Profile(p.clone());
                    if p.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Device(d) => {
                    let name = d.name.name.clone();
                    let exported_item = ExportedItem::Device(d.clone());
                    if d.is_exported {
                        exports.items.insert(name.clone(), exported_item.clone());
                    } else {
                        exports.private_symbols.insert(name.clone());
                    }
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Test(t) => {
                    let name = t.name.name.clone();
                    let exported_item = ExportedItem::Test(t.clone());
                    exports.items.insert(name.clone(), exported_item.clone());
                    local_decls.insert(name, exported_item);
                }
                TopLevelItem::Export(_) | TopLevelItem::Statement(_) | TopLevelItem::Impl(_) => {}
            }
        }

        // 2. Process all `export { Sym1, Sym2 }` re-export blocks
        for item in &program.items {
            if let TopLevelItem::Export(exp) = item {
                for sym in &exp.symbols {
                    // Check if it corresponds to a local declaration
                    if let Some(local_item) = local_decls.get(sym) {
                        exports.items.insert(sym.clone(), local_item.clone());
                        exports.private_symbols.remove(sym);
                    } else if let Some(def) = symbol_table.get_symbol(sym.as_str()) {
                        // Re-exported from a sub-import
                        exports
                            .items
                            .insert(sym.clone(), ExportedItem::ReExport(def));
                        exports.private_symbols.remove(sym);
                    }
                }
            }
        }

        exports
    }

    /// Check if a symbol is exported by name
    pub fn get(&self, name: &str) -> Option<&ExportedItem> {
        self.items.get(name)
    }

    /// Check if a symbol exists locally in the module but is private
    pub fn is_private(&self, name: &str) -> bool {
        self.private_symbols.contains(name) && !self.items.contains_key(name)
    }
}
