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

use crate::embedded_stdlib;
use crate::symbol_table::SymbolTable;
use compact_str::CompactString;
use hwc_parser::{Definition, Import, Lexer, ModulePath, Parser, Program};
use miette::{Diagnostic, SourceSpan};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during module resolution
#[derive(Error, Debug, Diagnostic)]
pub enum ResolverError {
    #[error("Cannot resolve import '{path}': file not found")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Verify the import path is correct. For standard library, use '@std/module/name'. For relative imports, ensure the file exists relative to the importing file.")
    )]
    FileNotFound {
        path: String,
        #[label("import statement")]
        span: Option<SourceSpan>,
    },

    #[error("Failed to read file '{path}': {error}")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Check file permissions and ensure the file is readable.")
    )]
    FileReadError {
        path: CompactString,
        error: String,
        #[label("import statement")]
        span: Option<SourceSpan>,
    },

    #[error("Failed to parse file '{path}': {error}")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Fix syntax errors in the imported file before importing it.")
    )]
    ParseError {
        path: CompactString,
        error: String,
        #[label("import statement")]
        span: Option<SourceSpan>,
    },

    #[error("Circular import detected: {chain}")]
    #[diagnostic(
        code(C22),
        url("https://docs.hw-script.org/errors/C22"),
        help("Remove the circular dependency by refactoring shared definitions into a separate file that both modules can import.")
    )]
    CircularImport {
        chain: String,
        #[label("import creates circular dependency")]
        span: Option<SourceSpan>,
    },

    #[error("External package @{org}/{name} not yet supported")]
    #[diagnostic(
        code(C21),
        url("https://docs.hw-script.org/errors/C21"),
        help("Only @std/ imports are currently available. External packages will be supported in future releases via the Hardware Package Manager (HPM).")
    )]
    ExternalPackageNotSupported {
        org: CompactString,
        name: String,
        #[label("unsupported package")]
        span: Option<SourceSpan>,
    },

    #[error("Standard library module '@std/{path}' not found")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Check the standard library documentation for available modules. Common modules: @std/primitives/units, @std/primitives/math, @std/materials/conductors")
    )]
    StdlibNotFound {
        path: String,
        #[label("unknown stdlib module")]
        span: Option<SourceSpan>,
    },

    #[error("Invalid import path")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Use @std/ for standard library, @org/package for external packages, or relative paths for local files (e.g., 'materials' or 'lib/utils').")
    )]
    InvalidImportPath {
        #[label("invalid path")]
        span: Option<SourceSpan>,
    },

    #[error("Symbol '{symbol}' not found in module '{path}'")]
    #[diagnostic(
        code(C25),
        url("https://docs.hw-script.org/errors/C25"),
        help("Check the module's exports or use 'import * from {path}' to see all available symbols. The symbol may be defined but not exported.")
    )]
    SymbolNotFound {
        symbol: String,
        path: String,
        #[label("symbol not found")]
        span: Option<SourceSpan>,
        #[source_code]
        src: miette::NamedSource,
    },

    #[error("Symbol '{symbol}' is not exported")]
    #[diagnostic(
        code(C26),
        url("https://docs.hw-script.org/errors/C26"),
        help("Only symbols marked with 'export' can be imported. Either:\n  1. Add 'export' to the symbol definition in '{path}'\n  2. Remove this symbol from your import list\n  3. Use a different public symbol from the module")
    )]
    PrivateSymbolAccess {
        symbol: String,
        path: String,
        #[label("private symbol cannot be imported")]
        span: Option<SourceSpan>,
        #[source_code]
        src: miette::NamedSource,
    },
}

impl ResolverError {
    /// Add span information to an error
    pub fn with_span(mut self, span: hwc_parser::Span) -> Self {
        let source_span = SourceSpan::new(span.start.into(), (span.end - span.start).into());
        match &mut self {
            Self::FileNotFound { span: s, .. }
            | Self::FileReadError { span: s, .. }
            | Self::ParseError { span: s, .. }
            | Self::CircularImport { span: s, .. }
            | Self::ExternalPackageNotSupported { span: s, .. }
            | Self::StdlibNotFound { span: s, .. }
            | Self::InvalidImportPath { span: s } => {
                *s = Some(source_span);
            }
            Self::SymbolNotFound { span: s, .. } | Self::PrivateSymbolAccess { span: s, .. } => {
                *s = Some(source_span);
            }
        }
        self
    }

    /// Add source code to an error for better diagnostics
    pub fn with_source(mut self, source: String, file_name: String) -> Self {
        let named_src = miette::NamedSource::new(file_name, source);
        match &mut self {
            Self::SymbolNotFound { src, .. } | Self::PrivateSymbolAccess { src, .. } => {
                *src = named_src;
            }
            _ => {}
        }
        self
    }
}

/// Module Resolver handles import resolution with clean separation of concerns:
/// - AST parsing is cached globally (pure, stateless optimization)
/// - Symbol registration happens per-import (no skipping, deterministic)
pub struct ModuleResolver {
    /// Path to the standard library directory
    stdlib_path: PathBuf,

    /// Pure AST cache: PathBuf → parsed Program
    /// This is ONLY for performance (avoid re-parsing). It has zero side effects
    /// and does not track whether symbols were registered.
    ast_cache: rustc_hash::FxHashMap<PathBuf, Program>,

    /// Stack for circular import detection (bounded, temporary)
    resolution_stack: Vec<PathBuf>,
}

impl ModuleResolver {
    /// Create a new module resolver
    pub fn new() -> Result<Self, ResolverError> {
        let stdlib_path = Self::find_stdlib_path()?;

        Ok(Self {
            stdlib_path,
            ast_cache: rustc_hash::FxHashMap::default(),
            resolution_stack: Vec::new(),
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
                span: None,
            })?;

        if !stdlib_path.exists() {
            return Err(ResolverError::StdlibNotFound {
                path: stdlib_path.display().to_string(),
                span: None,
            });
        }

        Ok(stdlib_path)
    }

    /// Resolve an import and register its definitions into the symbol table
    ///
    /// **Clean Architecture**: This method ALWAYS registers the requested symbols,
    /// even if the file was previously parsed. The AST cache is purely for performance.
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
        eprintln!("[IMPORT_DEBUG] Resolving import from source: {}", source_file.display());
        eprintln!("[IMPORT_DEBUG] Import path: {:?}", import.path);
        eprintln!("[IMPORT_DEBUG] Import targets: {:?}", import.targets);
        
        let file_path = self.resolve_path(&import.path, source_file)?;
        
        eprintln!("[IMPORT_DEBUG] Resolved to file path: {}", file_path.display());

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
        self.register_import_targets(&import.targets, &program, &file_path, import.alias.as_ref(), symbol_table)?;

        Ok(())
    }

    /// Register import targets into the symbol table
    /// 
    /// Always executes, even for previously-loaded files (no state pollution)
    fn register_import_targets(
        &self,
        targets: &hwc_parser::ImportTargets,
        program: &Program,
        file_path: &Path,
        alias: Option<&hwc_parser::Identifier>,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        // If this import has an alias, create a new HPM layer for the namespace
        if alias.is_some() {
            symbol_table.push_hpm_layer();
        }

        match targets {
            hwc_parser::ImportTargets::Star => {
                // Wildcard import: register all EXPORTED definitions
                eprintln!("[IMPORT_DEBUG] Registering all exported definitions from {}", file_path.display());
                for definition in &program.definitions {
                    if self.is_exported(definition) {
                        eprintln!("[IMPORT_DEBUG]   - Registering: {}", self.def_name(definition));
                        self.register_definition(definition, symbol_table)?;
                    }
                }
            }
            hwc_parser::ImportTargets::List(names) => {
                // Selective import: register only requested symbols
                eprintln!("[IMPORT_DEBUG] Selective import for names: {:?}", names);
                
                for name in names {
                    let name_str = name.as_str();
                    
                    // Find the definition in this module's definitions
                    let def = program.definitions.iter().find(|d| self.def_matches_name(d, name_str));
                    
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
                                src: miette::NamedSource::new("", ""),
                            });
                        }
                        
                        eprintln!("[IMPORT_DEBUG]   - Found and registering: {}", name_str);
                        self.register_definition(definition, symbol_table)?;
                    } else {
                        // Not found in definitions - check if it's re-exported
                        let is_reexported = program.re_exports.iter()
                            .any(|re| re.symbol.as_str() == name_str);
                        
                        if is_reexported {
                            // This symbol was imported by this module and re-exported
                            // It should already be in the symbol table from when we recursively
                            // resolved this module's imports (step 4 above)
                            eprintln!("[IMPORT_DEBUG]   - Symbol '{}' is re-exported (already registered from sub-imports)", name_str);
                            // No action needed - symbol is already in the table
                        } else {
                            return Err(ResolverError::SymbolNotFound {
                                symbol: name_str.to_string(),
                                path: file_path.display().to_string(),
                                span: Some(SourceSpan::new(
                                    name.span.start.into(),
                                    (name.span.end - name.span.start).into(),
                                )),
                                src: miette::NamedSource::new("", ""),
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
    fn is_exported(&self, definition: &Definition) -> bool {
        match definition {
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
            Definition::PolymorphicInterface(_) => true, // TODO: add is_exported field
            Definition::Space(_) => false, // Spaces are never exported
        }
    }

    /// Check if a definition matches a name
    fn def_matches_name(&self, definition: &Definition, name: &str) -> bool {
        match definition {
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
            Definition::Space(_) => false,
        }
    }

    /// Get a definition's name for debug output
    fn def_name(&self, definition: &Definition) -> String {
        match definition {
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
        }
    }

    /// Stateless AST loader - parses on cache miss, returns cached on hit
    fn get_or_parse_program(&mut self, path: &Path) -> Result<Program, ResolverError> {
        // Check cache first (pure, stateless)
        if let Some(cached_program) = self.ast_cache.get(path) {
            return Ok(cached_program.clone());
        }

        // Cache miss - load and parse
        if path.starts_with("@std/") {
            // Embedded stdlib
            let module_name = path.strip_prefix("@std/").unwrap().to_str().unwrap();
            let defs = self.load_stdlib_embedded(module_name)?;
            
            // Create a stub Program (stdlib has no imports/re-exports)
            let program = Program {
                imports: vec![],
                re_exports: vec![],
                definitions: defs,
                span: hwc_parser::Span::new(0, 0),
            };
            
            self.ast_cache.insert(path.to_path_buf(), program.clone());
            Ok(program)
        } else {
            // File system
            let program = self.parse_file(path)?;
            self.ast_cache.insert(path.to_path_buf(), program.clone());
            Ok(program)
        }
    }

    /// Parse a file from disk
    fn parse_file(&self, path: &Path) -> Result<Program, ResolverError> {
        let source = std::fs::read_to_string(path).map_err(|e| ResolverError::FileReadError {
            path: path.display().to_string().into(),
            error: e.to_string(),
            span: None,
        })?;

        let lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().map_err(|e| ResolverError::ParseError {
            path: path.display().to_string().into(),
            error: format!("{:?}", e),
            span: None,
        })?;

        let collector =
            crate::DiagnosticCollector::new_with_file(&source, &path.to_string_lossy(), 20);
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(ResolverError::ParseError {
                path: path.display().to_string().into(),
                error: collector.format_errors(),
                span: None,
            });
        }

        Ok(program)
    }

    /// Load stdlib module from embedded source (zero I/O)
    fn load_stdlib_embedded(&mut self, name: &str) -> Result<Vec<Definition>, ResolverError> {
        embedded_stdlib::get_stdlib_definitions(name)
            .ok_or_else(|| ResolverError::StdlibNotFound {
                path: name.into(),
                span: None,
            })
    }

    /// Register a definition into the symbol table (HPM layer)
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
                symbol_table.register_import_profile(profile.as_ref().clone());
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
                symbol_table.register_import_unit(unit.clone());
                Ok(())
            }
            Definition::Device(device) => {
                symbol_table.register_import_device(device.clone());
                Ok(())
            }
            Definition::Const(const_def) => {
                symbol_table.register_import_constant(const_def.clone());
                Ok(())
            }
            Definition::Shape(shape_def) => {
                symbol_table.register_import_shape(shape_def.clone());
                Ok(())
            }
            Definition::MaterialAlias(alias) => {
                symbol_table.register_import_material_alias(alias.clone());
                Ok(())
            }
            Definition::Space(_) => {
                // Space definitions in imported files are ignored
                Ok(())
            }
        }
    }

    /// Resolve a module path to a file path
    fn resolve_path(
        &self,
        path: &ModulePath,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        match path {
            ModulePath::Package { org, name } => {
                if org == "std" {
                    self.resolve_stdlib_path(name)
                } else {
                    Err(ResolverError::ExternalPackageNotSupported {
                        org: org.clone(),
                        name: name.clone(),
                        span: None,
                    })
                }
            }
            ModulePath::Relative(path_str) | ModulePath::Quoted(path_str) => {
                self.resolve_relative_path(path_str, source_file)
            }
        }
    }

    /// Resolve a path relative to the source file's directory
    fn resolve_relative_path(
        &self,
        path_str: &str,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        let source_dir = source_file
            .parent()
            .ok_or_else(|| ResolverError::FileNotFound {
                path: format!(
                    "Cannot determine parent directory of {}",
                    source_file.display()
                ),
                span: None,
            })?;

        let mut file_path = source_dir.join(path_str);

        if file_path.extension().is_none() {
            file_path.set_extension("hw");
        }

        let canonical_path = file_path
            .canonicalize()
            .map_err(|_| ResolverError::FileNotFound {
                path: format!("{} (relative to {})", path_str, source_file.display()),
                span: None,
            })?;

        Ok(canonical_path)
    }

    /// Resolve a standard library path
    fn resolve_stdlib_path(&self, name: &str) -> Result<PathBuf, ResolverError> {
        if embedded_stdlib::has_stdlib_module(name) {
            return Ok(PathBuf::from(format!("@std/{}", name)));
        }

        let file_path = self.stdlib_path.join(format!("{}.hw", name));

        if !file_path.exists() {
            return Err(ResolverError::FileNotFound {
                path: format!(
                    "@std/{} (not in embedded stdlib or at {})",
                    name,
                    file_path.display()
                ),
                span: None,
            });
        }

        Ok(file_path)
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
        };
        assert!(err.to_string().contains("test"));

        let err = ResolverError::StdlibNotFound {
            path: "stdlib".to_string(),
        };
        assert!(err.to_string().contains("stdlib"));
    }
}
