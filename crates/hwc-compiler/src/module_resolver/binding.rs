//! Import binding implementation (v0.3.1 Clean Architecture)
//!
//! Binds requested symbols from a target `ModuleExports` directly into the importer's `SymbolTable`.

use hwc_parser::ImportSymbols;
use miette::NamedSource;
use std::path::Path;

use super::errors::ResolverError;
use super::exports::ModuleExports;
use crate::symbol_table::SymbolTable;

/// Bind imported symbols from the resolved `ModuleExports` into `symbol_table`
pub fn bind_imports(
    symbols: &ImportSymbols,
    exports: &ModuleExports,
    file_path: &Path,
    symbol_table: &mut SymbolTable,
) -> Result<(), ResolverError> {
    match symbols {
        ImportSymbols::All => {
            for (_name, item) in &exports.items {
                symbol_table.register_exported_item(item);
            }
        }
        ImportSymbols::Named(names) => {
            for name in names {
                bind_single_symbol(name.as_str(), exports, file_path, symbol_table)?;
            }
        }
        ImportSymbols::Single(name) => {
            bind_single_symbol(name.as_str(), exports, file_path, symbol_table)?;
        }
    }

    Ok(())
}

fn bind_single_symbol(
    name: &str,
    exports: &ModuleExports,
    file_path: &Path,
    symbol_table: &mut SymbolTable,
) -> Result<(), ResolverError> {
    if let Some(item) = exports.get(name) {
        symbol_table.register_exported_item(item);
        Ok(())
    } else if exports.is_private(name) {
        Err(ResolverError::PrivateSymbolAccess {
            symbol: name.to_string(),
            path: file_path.display().to_string(),
            span: None,
            src: NamedSource::new("", ""),
        })
    } else {
        Err(ResolverError::SymbolNotFound {
            symbol: name.to_string(),
            path: file_path.display().to_string(),
            span: None,
            src: NamedSource::new("", ""),
        })
    }
}
