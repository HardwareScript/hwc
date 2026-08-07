//! Module Resolver for Import System (Gap 5.4)
//!
//! Clean Architecture (v0.2.0):
//! 1. **Stateless File Loading**: Parse files once, cache AST globally (zero side effects)
//! 2. **Per-Import Symbol Registration**: Every import always registers its requested symbols
//!
//! This design eliminates the "resolved set" antipattern that caused re-export failures.
//! Symbol registration is now properly separated from file parsing/caching.
//!
//! Features:
//! - Embedded stdlib for instant imports (no disk I/O)
//! - Circular import detection
//! - Pure AST caching (no registration state pollution)
//! - Deterministic symbol resolution

mod errors;
mod loading;
mod paths;
mod register_definition;
mod registration;

pub use errors::ResolverError;

use crate::symbol_table::SymbolTable;
use hwc_parser::{Import, Program};
use miette::SourceSpan;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

/// Module Resolver handles import resolution with clean separation of concerns:
/// - AST parsing is cached globally (pure, stateless optimization)
/// - Symbol registration happens per-import (no skipping, deterministic)
pub struct ModuleResolver {
    /// Path to the standard library directory
    stdlib_path: PathBuf,

    /// Pure AST cache: PathBuf → parsed Program
    /// This is ONLY for performance (avoid re-parsing). It has zero side effects
    /// and does not track whether symbols were registered.
    ast_cache: FxHashMap<PathBuf, Program>,

    /// Stack for circular import detection (bounded, temporary)
    resolution_stack: Vec<PathBuf>,
}

impl ModuleResolver {
    /// Create a new module resolver
    pub fn new() -> Result<Self, ResolverError> {
        let stdlib_path = Self::find_stdlib_path()?;

        Ok(Self {
            stdlib_path,
            ast_cache: FxHashMap::default(),
            resolution_stack: Vec::new(),
        })
    }

    /// Resolve an import and register its definitions into the symbol table
    ///
    /// **Clean Architecture**: This method ALWAYS registers the requested symbols,
    /// even if the file was previously parsed. The AST cache is purely for performance.
    pub fn resolve_import(
        &mut self,
        import: &Import,
        source_file: &Path,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        let file_path = self.resolve_path(&import.path, source_file)?;

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

        // 2. Load Program (uses cache if available, zero side effects)
        let program = self.get_or_parse_program(&file_path)?;

        // 3. Push to resolution stack before processing sub-imports
        self.resolution_stack.push(file_path.clone());

        // 4. Recursively resolve the module's own imports first
        // This ensures that re-exported symbols are available in the module's scope
        for sub_import in &program.imports {
            self.resolve_import(sub_import, &file_path, symbol_table)?;
        }

        // 5. Pop from resolution stack
        self.resolution_stack.pop();

        // 6. Register Symbols (ALWAYS EXECUTED - No Skipping)
        // This is the key fix: we always register requested symbols, regardless of
        // whether the file was previously loaded. Symbol registration is per-import,
        // not per-file.
        self.register_import_targets(
            &import.targets,
            &program,
            &file_path,
            import.alias.as_ref(),
            symbol_table,
        )?;

        Ok(())
    }

    /// Get the number of cached files
    pub fn cache_size(&self) -> usize {
        self.ast_cache.len()
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
