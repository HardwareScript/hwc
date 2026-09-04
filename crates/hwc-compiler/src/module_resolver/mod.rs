//! Module Resolver for Import System (Gap 5.4)
//!
//! Clean Architecture (v0.3.1):
//! 1. **Stateless File Loading**: Parse files once, cache AST globally (zero side effects)
//! 2. **Explicit ModuleExports**: Evaluates a module's public interface (`ModuleExports`)
//! 3. **Deterministic Import Binding**: Binds requested symbols directly into `SymbolTable`
//!
//! Features:
//! - Embedded stdlib for instant imports (no disk I/O)
//! - Circular import detection
//! - Strongly-typed export interface (`ModuleExports` / `ExportedItem`)
//! - Deterministic symbol resolution

mod binding;
mod errors;
mod exports;
mod loading;
mod paths;

pub use errors::ResolverError;
pub use exports::{ExportedItem, ModuleExports};

use crate::symbol_table::SymbolTable;
use hwc_parser::ImportDecl;
use miette::SourceSpan;
use std::path::{Path, PathBuf};

/// Module Resolver handles import resolution with clean separation of concerns:
/// - Files are parsed fresh each time (no stale cache)
/// - Modules expose a strongly-typed `ModuleExports` interface
/// - Symbol registration happens per-import deterministically
pub struct ModuleResolver {
    /// Path to the standard library directory
    stdlib_path: PathBuf,

    /// Stack for circular import detection (bounded, temporary)
    resolution_stack: Vec<PathBuf>,
}

impl ModuleResolver {
    /// Create a new module resolver
    pub fn new() -> Result<Self, ResolverError> {
        let stdlib_path = Self::find_stdlib_path()?;

        Ok(Self {
            stdlib_path,
            resolution_stack: Vec::new(),
        })
    }

    /// Resolve an import and register its definitions into the symbol table
    pub fn resolve_import(
        &mut self,
        import: &ImportDecl,
        source_file: &Path,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        let file_path = self.resolve_import_path(&import.from, source_file)?;

        // 1. Circular Import Detection
        if self.resolution_stack.contains(&file_path) {
            let chain = self
                .resolution_stack
                .iter()
                .map(|p| p.display().to_string())
                .chain(std::iter::once(file_path.display().to_string()))
                .collect::<Vec<_>>()
                .join(" → ");
            return Err(ResolverError::CircularImport {
                chain,
                span: Some(SourceSpan::new(
                    import.span.start.into(),
                    (import.span.end - import.span.start).into(),
                )),
            });
        }

        // 2. Load Program (parse fresh each time)
        let program = self.parse_program(&file_path)?;

        // 3. Push to resolution stack before processing sub-imports
        self.resolution_stack.push(file_path.clone());

        // 4. Recursively resolve the module's own imports first
        // This ensures that re-exported symbols are available in the module's scope
        for sub_import in &program.imports {
            self.resolve_import(sub_import, &file_path, symbol_table)?;
        }

        // 5. Pop from resolution stack
        self.resolution_stack.pop();

        // 6. Build ModuleExports interface and bind requested symbols into SymbolTable
        let exports = ModuleExports::from_program(&program, symbol_table);
        binding::bind_imports(&import.symbols, &exports, &file_path, symbol_table)?;

        Ok(())
    }
}

impl Default for ModuleResolver {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ModuleResolver")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_stdlib_path() {
        let result = ModuleResolver::find_stdlib_path();
        assert!(result.is_ok(), "Should find stdlib path");

        let path = result.unwrap();
        assert!(path.exists(), "Stdlib path should exist");
        assert!(path.ends_with("stdlib"), "Path should end with 'stdlib'");
    }

    #[test]
    fn test_find_stdlib_path_returns_pathbuf() {
        let result = ModuleResolver::find_stdlib_path();

        match result {
            Ok(path) => {
                assert!(path.to_str().is_some(), "Path should be valid UTF-8");
            }
            Err(e) => {
                assert!(matches!(e, ResolverError::StdlibNotFound { .. }));
            }
        }
    }

    #[test]
    fn test_resolver_error_types() {
        let err = ResolverError::FileNotFound {
            path: "test".to_string(),
            span: None,
        };
        assert!(err.to_string().contains("test"));

        let err = ResolverError::StdlibNotFound {
            path: "stdlib".to_string(),
            span: None,
        };
        assert!(err.to_string().contains("stdlib"));
    }
}
