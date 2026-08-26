//! Parser error types with miette diagnostics

use crate::lexer::Span;
use compact_str::CompactString;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[allow(dead_code)]
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parser errors with authoritative, professional miette diagnostics
#[derive(Debug, Clone, Error, Diagnostic)]
pub enum ParseError {
    #[error("Unexpected {found}")]
    #[diagnostic(
        code(S14),
        url("https://docs.hw-script.org/errors/S14"),
        help("Expected {expected}")
    )]
    UnexpectedToken {
        #[label("{expected} required here")]
        span: SourceSpan,
        expected: CompactString,
        found: CompactString,
    },

    // Phase 1.1 — Break up S14 into specific error codes (S30-S37)
    #[error("{message}")]
    #[diagnostic(
        code(S30),
        url("https://docs.hw-script.org/errors/S30"),
        help("Use ':' to separate property names from values (The Boundary Law: ':' for properties, '=' for logic)")
    )]
    ExpectedColon {
        #[label("Expected ':' here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S31),
        url("https://docs.hw-script.org/errors/S31"),
        help("This field requires a quoted string, e.g., `technology: \"ASIC\"`")
    )]
    ExpectedQuotedString {
        #[label("Expected quoted string here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S32),
        url("https://docs.hw-script.org/errors/S32"),
        help("Expected a name here, e.g., `component Name:`")
    )]
    ExpectedIdentifier {
        #[label("Expected identifier here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S33),
        url("https://docs.hw-script.org/errors/S33"),
        help("Expected a value, number, measurement, or variable")
    )]
    ExpectedExpression {
        #[label("Expected expression here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S34),
        url("https://docs.hw-script.org/errors/S34"),
        help("Each statement must be on its own line")
    )]
    ExpectedNewline {
        #[label("Expected newline here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S35),
        url("https://docs.hw-script.org/errors/S35"),
        help("This block requires increased indentation (4 spaces per level)")
    )]
    ExpectedIndent {
        #[label("Expected increased indentation here")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S36),
        url("https://docs.hw-script.org/errors/S36"),
        help("Missing closing delimiter for this block")
    )]
    ExpectedClosingDelimiter {
        #[label("Missing closing delimiter")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S37),
        url("https://docs.hw-script.org/errors/S37"),
        help("List valid keywords for this context")
    )]
    ExpectedPropertyKeyword {
        #[label("Unknown keyword here")]
        span: SourceSpan,
        message: CompactString,
    },

    // Phase 1.2 — Additional specific error codes (S40-S43)
    #[error("{message}")]
    #[diagnostic(
        code(S40),
        url("https://docs.hw-script.org/errors/S40"),
        help("List valid fields for this block type")
    )]
    UnknownField {
        #[label("Unrecognized field name")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S41),
        url("https://docs.hw-script.org/errors/S41"),
        help("{message}")
    )]
    InvalidSyntax {
        #[label("Syntax error")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S42),
        url("https://docs.hw-script.org/errors/S42"),
        help("Migration instructions: see documentation for updated syntax")
    )]
    DeprecatedSyntax {
        #[label("Removed or renamed syntax")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("{message}")]
    #[diagnostic(
        code(S43),
        url("https://docs.hw-script.org/errors/S43"),
        help("Expression syntax guidance: check operator precedence and operand placement")
    )]
    InvalidExpression {
        #[label("Malformed expression")]
        span: SourceSpan,
        message: CompactString,
    },

    #[error("File ended unexpectedly")]
    #[diagnostic(
        code(S15),
        url("https://docs.hw-script.org/errors/S15"),
        help("Check for missing closing brackets or incomplete statements")
    )]
    UnexpectedEof {
        #[label("File ended here")]
        span: SourceSpan,
    },

    #[error("{message}")]
    #[diagnostic(code(S99), url("https://docs.hw-script.org/errors/S99"))]
    General {
        #[label("{message}")]
        span: SourceSpan,
        message: CompactString,
    },

    // v0.1.6 Context-Aware Error Messages
    #[error("Expected ':' in property block")]
    #[diagnostic(
        code(S20),
        url("https://docs.hw-script.org/errors/S20"),
        help("The Boundary Law: Use ':' for declarative properties, '=' for behavioral logic.\nExample: resistance: 10kΩ")
    )]
    ExpectedColonInProperty {
        #[label("Use ':' here (not '=')")]
        span: SourceSpan,
    },

    #[error("Expected identifier (no quotes needed)")]
    #[diagnostic(
        code(S23),
        url("https://docs.hw-script.org/errors/S23"),
        help("{help_message}")
    )]
    ExpectedIdentifierNotString {
        #[label("Remove quotes")]
        span: SourceSpan,
        help_message: String,
    },

    #[error("The 'define' keyword was removed")]
    #[diagnostic(
        code(S24),
        url("https://docs.hw-script.org/errors/S24"),
        help("{help_message}")
    )]
    DefineKeywordRemoved {
        #[label("Remove 'define' keyword")]
        span: SourceSpan,
        help_message: String,
    },

    #[error("'%' cannot be used as a binary operator")]
    #[diagnostic(
        code(S27),
        url("https://docs.hw-script.org/errors/S27"),
        help("The '%' symbol is reserved for unit suffixes (e.g., 5%, 10%).\nFor modulo operation, use the 'mod' keyword instead.\nExample: count mod 10")
    )]
    PercentAsOperator {
        #[label("Use 'mod' keyword here")]
        span: SourceSpan,
    },
}

/// Convert our Span to miette's SourceSpan
pub(crate) fn span_to_source_span(span: &Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), (span.end - span.start).into())
}

// Context-Aware Error Helpers for v0.1.6
#[allow(dead_code)]
pub(crate) fn error_expected_colon_in_property(span: &Span) -> ParseError {
    ParseError::ExpectedColonInProperty {
        span: span_to_source_span(span),
    }
}

#[allow(dead_code)]
pub(crate) fn error_expected_identifier_not_string(span: &Span) -> ParseError {
    ParseError::ExpectedIdentifierNotString {
        span: span_to_source_span(span),
        help_message: format!(
            "v{} uses bare identifiers for type names.\nExample: component Resistor: (not component \"Resistor\":)",
            VERSION
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn error_define_keyword_removed(span: &Span) -> ParseError {
    ParseError::DefineKeywordRemoved {
        span: span_to_source_span(span),
        help_message: format!(
            "Type keywords are now first-class in v{}. Use them directly.\nMigration: define component \"Name\": → component Name:",
            VERSION
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn error_percent_as_operator(span: &Span) -> ParseError {
    ParseError::PercentAsOperator {
        span: span_to_source_span(span),
    }
}
