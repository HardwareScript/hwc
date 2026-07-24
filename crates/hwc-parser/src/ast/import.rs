//! Import and re-export statement types

use serde::{Deserialize, Serialize};

use super::common::Identifier;
use crate::lexer::Span;
use compact_str::CompactString;

/// Import statement: `import X from Y` (v0.1.6)
///
/// Supports three import modes (GAP3):
/// 1. Selective: `import A, B, C from @path`
/// 2. Namespace: `import @path as Alias`
/// 3. Wildcard: `import * from @path`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Import {
    pub targets: ImportTargets,
    pub path: ModulePath,
    pub alias: Option<Identifier>,
    pub span: Span,
}

/// Re-export statement: `export X` (v0.2.0 Explicit Re-Exports)
///
/// Rust-style explicit re-export of imported symbols.
/// Makes an imported symbol available to downstream importers.
///
/// Example:
/// ```hw
/// import PublicSilicon, Aluminum from materials
/// 
/// # Re-export these materials so they're part of this file's public API
/// export PublicSilicon
/// export Aluminum
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReExport {
    pub symbol: Identifier,
    pub span: Span,
}

/// Import targets: what to import from the module
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportTargets {
    /// Wildcard import: `import * from @path`
    Star,
    /// Selective import: `import A, B, C from @path`
    List(Vec<Identifier>),
}

/// Module path: `logic/adders`, `@robotics/motor`, or `"Custom Path/Board.hw"`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModulePath {
    // REMOVED (pre-release): Standard(Vec<...>) variant for legacy `standard.materials` dot syntax.
    // Reason removed: Dot syntax was pre-v0.1.6 import style, inconsistent with / paths and @package.
    // Resolver already errored on it; parser now rejects at parse time. Avoid future: don't support
    // multiple syntaxes for same concept "temporarily"; standardize early.
    /// Package registry path: `@robotics/motor`
    Package { org: CompactString, name: String },
    /// Relative path with bare identifiers: `logic/adders` (v0.1.6)
    Relative(String),
    /// Quoted path for paths with spaces: `"Custom Path/Board.hw"` (v0.1.6)
    Quoted(String),
}
