//! Symbol table error types

use crate::logic_synthesizer::SynthesisError;
use compact_str::CompactString;
use miette::Diagnostic;
use thiserror::Error;

/// Symbol table errors
#[derive(Error, Debug, Diagnostic)]
pub enum SymbolError {
    #[error("Duplicate {kind} definition: '{name}'")]
    #[diagnostic(help("A {kind} named '{name}' is already defined. Use a different name."))]
    DuplicateDefinition {
        name: CompactString,
        kind: &'static str,
        #[label("duplicate definition here")]
        span: (usize, usize),
        #[label("first defined here")]
        first_span: Option<(usize, usize)>,
    },

    #[error("Undefined {kind}: '{name}'")]
    #[diagnostic(help("Make sure the {kind} is defined before using it, or check for typos."))]
    UndefinedSymbol {
        name: CompactString,
        kind: &'static str,
        #[label("used here")]
        span: Option<(usize, usize)>,
    },

    #[error(transparent)]
    #[diagnostic(transparent)]
    LogicValidationError(Box<SynthesisError>),

    /// Warning: Local definition shadows an imported definition (Rule 1: Local Beats Global)
    #[error("Local {kind} '{name}' shadows imported definition")]
    #[diagnostic(
        severity(Warning),
        help("Rename local definition to avoid confusion, or remove the import if you intend to override")
    )]
    ImportShadowing {
        name: CompactString,
        kind: &'static str,
        #[label("local definition shadows import")]
        span: (usize, usize),
        import_source: CompactString,
    },

    #[error("Circular material alias detected: '{name}' -> '{target}'")]
    #[diagnostic(help("Remove the circular dependency in your material aliases."))]
    CircularAlias {
        name: CompactString,
        target: CompactString,
        #[label("circular alias defined here")]
        span: (usize, usize),
    },

    #[error("Material alias depth exceeded for '{name}': {depth} hops")]
    #[diagnostic(help("Material aliases are limited to 10 hops to prevent infinite recursion."))]
    AliasDepthExceeded { name: CompactString, depth: usize },
}

impl From<SynthesisError> for SymbolError {
    fn from(err: SynthesisError) -> Self {
        Self::LogicValidationError(Box::new(err))
    }
}
