//! Resolver error types and span/source helpers.

use compact_str::CompactString;
use hwc_parser::Span;
use miette::{Diagnostic, NamedSource, SourceSpan};
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
        src: NamedSource,
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
        src: NamedSource,
    },
}

impl ResolverError {
    /// Add span information to an error
    pub fn with_span(mut self, span: Span) -> Self {
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
        let named_src = NamedSource::new(file_name, source);
        match &mut self {
            Self::SymbolNotFound { src, .. } | Self::PrivateSymbolAccess { src, .. } => {
                *src = named_src;
            }
            _ => {}
        }
        self
    }
}
