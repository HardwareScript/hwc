use compact_str::CompactString;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

fn span_of(start: usize, end: usize) -> SourceSpan {
    SourceSpan::new(start.into(), (end - start).into())
}

fn opt_span_of(start: usize, end: usize) -> Option<SourceSpan> {
    Some(span_of(start, end))
}

/// Symbol table errors
#[derive(Error, Debug, Diagnostic)]
pub enum SymbolError {
    #[error("Duplicate {kind} definition: '{name}'")]
    #[diagnostic(help("A {kind} named '{name}' is already defined. Use a different name."))]
    DuplicateDefinition {
        name: CompactString,
        kind: &'static str,
        #[label("duplicate definition here")]
        span: SourceSpan,
        #[label("first defined here")]
        first_span: Option<SourceSpan>,
    },

    #[error("Undefined {kind}: '{name}'")]
    #[diagnostic(help("Make sure the {kind} is defined before using it, or check for typos."))]
    UndefinedSymbol {
        name: CompactString,
        kind: &'static str,
        #[label("used here")]
        span: Option<SourceSpan>,
    },

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
        span: SourceSpan,
        import_source: CompactString,
    },

    #[error("Circular material alias detected: '{name}' -> '{target}'")]
    #[diagnostic(help("Remove the circular dependency in your material aliases."))]
    CircularAlias {
        name: CompactString,
        target: CompactString,
        #[label("circular alias defined here")]
        span: SourceSpan,
    },

    #[error("Material alias depth exceeded for '{name}': {depth} hops")]
    #[diagnostic(help("Material aliases are limited to 10 hops to prevent infinite recursion."))]
    AliasDepthExceeded { name: CompactString, depth: usize },

    #[error("Type mismatch for '{name}': expected {expected}, found {found}")]
    #[diagnostic(help(
        "The symbol '{name}' is defined as a {found}, but you're trying to use it as a {expected}."
    ))]
    TypeMismatch {
        name: CompactString,
        expected: &'static str,
        found: &'static str,
    },
}

impl SymbolError {
    /// Construct a DuplicateDefinition with (start, end) tuple spans.
    pub fn duplicate(
        name: CompactString,
        kind: &'static str,
        span: (usize, usize),
        first_span: Option<(usize, usize)>,
    ) -> Self {
        Self::DuplicateDefinition {
            name,
            kind,
            span: span_of(span.0, span.1),
            first_span: first_span.and_then(|s| opt_span_of(s.0, s.1)),
        }
    }

    /// Construct an UndefinedSymbol with an optional (start, end) tuple span.
    pub fn undefined(
        name: CompactString,
        kind: &'static str,
        span: Option<(usize, usize)>,
    ) -> Self {
        Self::UndefinedSymbol {
            name,
            kind,
            span: span.and_then(|s| opt_span_of(s.0, s.1)),
        }
    }

    /// Construct an ImportShadowing with (start, end) tuple span.
    pub fn shadowing(
        name: CompactString,
        kind: &'static str,
        span: (usize, usize),
        import_source: CompactString,
    ) -> Self {
        Self::ImportShadowing {
            name,
            kind,
            span: span_of(span.0, span.1),
            import_source,
        }
    }

    /// Construct a CircularAlias with (start, end) tuple span.
    pub fn circular(name: CompactString, target: CompactString, span: (usize, usize)) -> Self {
        Self::CircularAlias {
            name,
            target,
            span: span_of(span.0, span.1),
        }
    }

    /// Construct a TypeMismatch error
    pub fn type_mismatch(name: &str, expected: &'static str, found: &'static str) -> Self {
        Self::TypeMismatch {
            name: name.into(),
            expected,
            found,
        }
    }
}
