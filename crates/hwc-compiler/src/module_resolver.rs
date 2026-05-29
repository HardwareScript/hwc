//! Module Resolver for Import System (Gap 5.4)
//!
//! Handles resolution and loading of imported modules from:
//! - Standard library: `@std/logic/gates` → embedded in binary (zero I/O)
//! - Local files: relative paths → disk lookup with caching
//! - External packages: `@org/package` → package registry (future)
//!
//! Features:
//! - Embedded stdlib for instant imports (no disk I/O)
//! - Circular import detection
//! - Import caching to avoid re-parsing
//! - Local file override (local files take precedence over stdlib)
//! - Proper error messages with file paths

use crate::embedded_stdlib;
use crate::symbol_table::SymbolTable;
use compact_str::CompactString;
use hwc_parser::{Definition, Import, Lexer, ModulePath, Parser};
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during module resolution
#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("Cannot resolve import '{path}': file not found")]
    FileNotFound { path: String },

    #[error("Failed to read file '{path}': {error}")]
    FileReadError { path: CompactString, error: String },

    #[error("Failed to parse file '{path}': {error}")]
    ParseError { path: CompactString, error: String },

    #[error("Circular import detected: {chain}")]
    CircularImport { chain: String },

    #[error("External package @{org}/{name} not yet supported. Only @std/ imports are currently available.")]
    ExternalPackageNotSupported { org: CompactString, name: String },

    #[error("Standard library module '@std/{path}' not found")]
    StdlibNotFound { path: String },

    #[error("Invalid import path. Use @std/ for standard library or @org/package for external packages.")]
    InvalidImportPath,
}

/// Module Resolver handles import resolution and caching
pub struct ModuleResolver {
    /// Path to the standard library directory
    stdlib_path: PathBuf,

    /// Cache of already-parsed files (path → parsed definitions)
    cache: rustc_hash::FxHashMap<PathBuf, Vec<Definition>>,

    /// Stack of currently-being-resolved imports (for circular detection)
    resolution_stack: Vec<PathBuf>,

    /// Set of all resolved imports (for quick lookup)
    resolved: FxHashSet<PathBuf>,
}

impl ModuleResolver {
    /// Create a new module resolver
    ///
    /// The stdlib path is determined relative to the compiler crate:
    /// `hwc/crates/hwc-compiler/../../stdlib`
    pub fn new() -> Result<Self, ResolverError> {
        let stdlib_path = Self::find_stdlib_path()?;

        Ok(Self {
            stdlib_path,
            cache: rustc_hash::FxHashMap::default(),
            resolution_stack: Vec::new(),
            resolved: FxHashSet::default(),
        })
    }

    /// Find the standard library path relative to the compiler crate
    fn find_stdlib_path() -> Result<PathBuf, ResolverError> {
        let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let stdlib_path = compiler_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("stdlib"))
            .ok_or_else(|| ResolverError::StdlibNotFound {
                path: "stdlib directory".into(),
            })?;

        if !stdlib_path.exists() {
            return Err(ResolverError::StdlibNotFound {
                path: stdlib_path.display().to_string(),
            });
        }

        Ok(stdlib_path)
    }

    /// Resolve an import and register its definitions into the symbol table
    ///
    /// This is the main entry point for import resolution.
    ///
    /// # Arguments
    /// * `import` - The import statement to resolve
    /// * `source_file` - Path to the file containing this import (for relative resolution)
    /// * `symbol_table` - Symbol table to register definitions into
    pub fn resolve_import(
        &mut self,
        import: &Import,
        source_file: &Path,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        let file_path = self.resolve_path(&import.path, source_file)?;

        // Check if already resolved
        if self.resolved.contains(&file_path) {
            // eprintln!($3"[DEBUG] Import already resolved: {}", file_path.display());
            return Ok(());
        }

        // Check for circular imports
        if self.resolution_stack.contains(&file_path) {
            let chain = self
                .resolution_stack
                .iter()
                .map(|p| p.display().to_string())
                .chain(std::iter::once(file_path.display().to_string()))
                .collect::<Vec<_>>()
                .join(" → ");
            return Err(ResolverError::CircularImport { chain });
        }

        // Add to resolution stack
        self.resolution_stack.push(file_path.clone());

        // Load definitions (embedded stdlib or disk)
        let definitions = if file_path.starts_with("@std/") {
            // Extract module name from synthetic path
            let module_name = file_path.strip_prefix("@std/").unwrap().to_str().unwrap();
            // eprintln!($3"[DEBUG] Loading from embedded stdlib: {}", module_name);
            self.load_stdlib_embedded(module_name)?
        } else {
            // eprintln!($3"[DEBUG] Loading from disk: {}", file_path.display());
            self.load_file(&file_path)?
        };

        // If this import has an alias, create a new HPM layer for the namespace
        if import.alias.is_some() {
            symbol_table.push_hpm_layer();
        }

        // Register definitions based on import mode
        match &import.targets {
            hwc_parser::ImportTargets::Star => {
                // Wildcard import: register all definitions
                for definition in &definitions {
                    self.register_definition(definition, symbol_table)?;
                }
            }
            hwc_parser::ImportTargets::List(names) => {
                // Selective import: only register requested definitions
                for name in names {
                    let name_str = name.as_str();

                    // Find the definition with this name
                    let def = definitions.iter().find(|d| match d {
                        Definition::Material(m) => m.name.as_str() == name_str,
                        Definition::Profile(p) => p.name.as_str() == name_str,
                        Definition::Component(c) => c.name.as_str() == name_str,
                        Definition::Module(m) => m.name.as_str() == name_str,
                        Definition::Logic(l) => l.name.as_str() == name_str,
                        Definition::Enum(e) => e.name.as_str() == name_str,
                        Definition::Struct(s) => s.name.as_str() == name_str,
                        Definition::Mechanical(m) => m.name.as_str() == name_str,
                        Definition::Interface(i) => i.name.as_str() == name_str,
                        Definition::Test(t) => t.name.as_str() == name_str,
                        Definition::SignalGroup(sg) => sg.name.as_str() == name_str,
                        Definition::Pattern(p) => p.name.as_str() == name_str,
                        Definition::Strategy(s) => s.name.as_str() == name_str,
                        Definition::Unit(u) => u.symbol.as_str() == name_str,
                        Definition::Device(d) => d.name.as_str() == name_str,
                        Definition::Const(c) => c.name.as_str() == name_str,
                        _ => false,
                    });

                    if let Some(definition) = def {
                        self.register_definition(definition, symbol_table)?;
                    } else {
                        return Err(ResolverError::FileNotFound {
                            path: format!(
                                "{} (definition '{}' not found in module)",
                                file_path.display(),
                                name_str
                            ),
                        });
                    }
                }
            }
        }

        // Register the namespace alias if present
        if let Some(alias) = &import.alias {
            symbol_table.register_namespace_alias(alias.as_str().to_string().into());
        }

        // Mark as resolved
        self.resolved.insert(file_path.clone());

        // Remove from resolution stack
        self.resolution_stack.pop();

        Ok(())
    }

    /// Resolve a module path to a file path
    ///
    /// # Arguments
    /// * `path` - The module path from the import statement
    /// * `source_file` - Path to the file containing this import (for relative resolution)
    fn resolve_path(
        &self,
        path: &ModulePath,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        // Legacy ModulePath::Standard (dot syntax for standard.materials) fully removed pre-release.
        // Parser rejects it; no match arm remains. See hwc-parser/src/ast/import.rs for removal rationale.
        match path {
            ModulePath::Package { org, name } => {
                // Handle @std/ imports
                if org == "std" {
                    self.resolve_stdlib_path(name)
                } else {
                    // External packages not yet supported
                    Err(ResolverError::ExternalPackageNotSupported {
                        org: org.clone(),
                        name: name.clone(),
                    })
                }
            }
            ModulePath::Relative(path_str) => {
                // Bare identifier path: materials, logic/adders
                // Resolve relative to the source file's directory
                self.resolve_relative_path(path_str, source_file)
            }
            ModulePath::Quoted(path_str) => {
                // Quoted path: "Custom Path/Board.hw"
                // Resolve relative to the source file's directory
                self.resolve_relative_path(path_str, source_file)
            }
        }
    }

    /// Resolve a path relative to the source file's directory
    ///
    /// Examples (assuming source file is at `/project/src/main.hw`):
    /// - `materials` → `/project/src/materials.hw`
    /// - `lib/utils` → `/project/src/lib/utils.hw`
    /// - `../common/types` → `/project/common/types.hw`
    fn resolve_relative_path(
        &self,
        path_str: &str,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        // Get the directory containing the source file
        let source_dir = source_file
            .parent()
            .ok_or_else(|| ResolverError::FileNotFound {
                path: format!(
                    "Cannot determine parent directory of {}",
                    source_file.display()
                ),
            })?;

        // Build the relative path
        let mut file_path = source_dir.join(path_str);

        // Add .hw extension if not present
        if file_path.extension().is_none() {
            file_path.set_extension("hw");
        }

        // Canonicalize to resolve .. and . components
        let canonical_path = file_path
            .canonicalize()
            .map_err(|_| ResolverError::FileNotFound {
                path: format!("{} (relative to {})", path_str, source_file.display()),
            })?;

        Ok(canonical_path)
    }

    /// Resolve a standard library path
    ///
    /// Strategy:
    /// 1. Try embedded stdlib first (instant, zero I/O)
    /// 2. Fall back to disk for development/override scenarios
    ///
    /// Examples:
    /// - `logic/gates` → embedded stdlib (instant)
    /// - `materials` → embedded stdlib (instant)
    fn resolve_stdlib_path(&self, name: &str) -> Result<PathBuf, ResolverError> {
        // Check if module exists in embedded stdlib
        if embedded_stdlib::has_stdlib_module(name) {
            // Return a synthetic path for tracking (actual source is embedded)
            // We use a special marker to indicate this is embedded
            return Ok(PathBuf::from(format!("@std/{}", name)));
        }

        // Fall back to disk-based stdlib (for development or user overrides)
        let file_path = self.stdlib_path.join(format!("{}.hw", name));

        if !file_path.exists() {
            return Err(ResolverError::FileNotFound {
                path: format!(
                    "@std/{} (not in embedded stdlib or at {})",
                    name,
                    file_path.display()
                ),
            });
        }

        Ok(file_path)
    }

    /// Load and parse a file, using cache if available
    fn load_file(&mut self, path: &Path) -> Result<Vec<Definition>, ResolverError> {
        // Check cache first
        if let Some(definitions) = self.cache.get(path) {
            // eprintln!($3"[DEBUG] Cache hit for {}", path.display());
            return Ok(definitions.clone());
        }

        // eprintln!($3"[DEBUG] Cache miss for {}, loading...", path.display());

        // Read the file
        let source = std::fs::read_to_string(path).map_err(|e| ResolverError::FileReadError {
            path: path.display().to_string().into(),
            error: e.to_string(),
        })?;

        // Parse the file
        let lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().map_err(|e| ResolverError::ParseError {
            path: path.display().to_string().into(),
            error: format!("{:?}", e),
        })?;

        let collector = crate::DiagnosticCollector::new(&source, &path.to_string_lossy(), 20);
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(ResolverError::ParseError {
                path: path.display().to_string().into(),
                error: collector.summary().to_string(),
            });
        }

        // Cache the definitions
        self.cache
            .insert(path.to_path_buf(), program.definitions.clone());

        Ok(program.definitions)
    }

    /// Load stdlib module from embedded source (zero I/O)
    fn load_stdlib_embedded(&mut self, name: &str) -> Result<Vec<Definition>, ResolverError> {
        // eprintln!($3"[DEBUG] Loading embedded stdlib: {}", name);

        // Get pre-parsed definitions from embedded cache
        embedded_stdlib::get_stdlib_definitions(name)
            .ok_or_else(|| ResolverError::StdlibNotFound { path: name.into() })
    }

    /// Register a definition into the symbol table
    fn register_definition(
        &self,
        definition: &Definition,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        // CRITICAL: Imported definitions go into the HPM layer, not the local layer
        // This enables the Authority Stack (Local > HPM > Prelude > Core)
        match definition {
            Definition::Material(mat) => {
                symbol_table.register_import_material(mat.clone());
                Ok(())
            }
            Definition::Profile(profile) => {
                symbol_table.register_import_profile(profile.clone());
                Ok(())
            }
            Definition::Component(component) => {
                symbol_table.register_import_component(component.clone());
                Ok(())
            }
            Definition::Module(module) => {
                symbol_table.register_import_module(module.clone());
                Ok(())
            }
            Definition::Logic(logic_def) => {
                symbol_table.register_import_logic(logic_def.clone());
                Ok(())
            }
            Definition::Enum(enum_def) => {
                symbol_table.register_import_enum(enum_def.clone());
                Ok(())
            }
            Definition::Struct(struct_def) => {
                symbol_table.register_import_struct(struct_def.clone());
                Ok(())
            }
            Definition::Mechanical(mechanical) => {
                symbol_table.register_import_mechanical(mechanical.clone());
                Ok(())
            }
            Definition::Interface(interface) => {
                symbol_table.register_import_interface(interface.clone());
                Ok(())
            }
            Definition::PolymorphicInterface(_poly_interface) => {
                // TODO: Register polymorphic interfaces in symbol table
                // For now, skip - will be implemented in interface validator
                Ok(())
            }
            Definition::Test(test) => {
                symbol_table.register_import_test(test.clone());
                Ok(())
            }
            Definition::SignalGroup(signal_group) => {
                symbol_table.register_import_signal_group(signal_group.clone());
                Ok(())
            }
            Definition::Pattern(pattern) => {
                symbol_table.register_import_pattern(pattern.clone());
                Ok(())
            }
            Definition::Strategy(strategy) => {
                symbol_table.register_import_strategy(strategy.clone());
                Ok(())
            }
            Definition::Unit(unit) => {
                // Register imported units in the HPM layer
                symbol_table.register_import_unit(unit.clone());
                Ok(())
            }
            Definition::Device(device) => {
                // Register imported devices in the HPM layer
                symbol_table.register_import_device(device.clone());
                Ok(())
            }
            Definition::Const(const_def) => {
                // Register imported constants in the HPM layer
                symbol_table.register_import_constant(const_def.clone());
                Ok(())
            }
            Definition::MaterialAlias(alias) => {
                // Register material aliases in symbol table
                symbol_table.register_import_material_alias(alias.clone());
                Ok(())
            }
            Definition::Space(_) => {
                // Space definitions in imported files are ignored
                Ok(())
            }
        }
    }

    /// Get the number of cached files
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Get the number of resolved imports
    pub fn resolved_count(&self) -> usize {
        self.resolved.len()
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
        // Simple test: verify the function returns a PathBuf without hanging
        let result = ModuleResolver::find_stdlib_path();

        // We don't care if it succeeds or fails, just that it completes quickly
        match result {
            Ok(path) => {
                assert!(path.to_str().is_some(), "Path should be valid UTF-8");
            }
            Err(e) => {
                // Expected when stdlib directory doesn't exist
                assert!(matches!(e, ResolverError::StdlibNotFound { .. }));
            }
        }
    }

    #[test]
    fn test_resolver_error_types() {
        // Test that error types can be constructed (no I/O)
        let err = ResolverError::FileNotFound {
            path: "test".to_string(),
        };
        assert!(err.to_string().contains("test"));

        let err = ResolverError::StdlibNotFound {
            path: "stdlib".to_string(),
        };
        assert!(err.to_string().contains("stdlib"));
    }
}
