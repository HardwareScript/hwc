//! Parser error types with miette diagnostics

use crate::lexer::Span;
use compact_str::CompactString;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

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

    #[error("Expected '=' in logic block")]
    #[diagnostic(
        code(S21),
        url("https://docs.hw-script.org/errors/S21"),
        help("The Boundary Law: Use '=' for assignments and comparisons in logic blocks.\nExample: count = count + 1")
    )]
    ExpectedEqualsInLogic {
        #[label("Use '=' here (not ':')")]
        span: SourceSpan,
    },

    #[error("Use single '=' for comparison")]
    #[diagnostic(
        code(S22),
        url("https://docs.hw-script.org/errors/S22"),
        help("Hardware Script uses single '=' for both assignment and comparison.\nContext determines meaning: standalone = assignment, after if/match = comparison.\nExample: if count = 0:")
    )]
    UsesSingleEqualsForComparison {
        #[label("Replace '==' with '='")]
        span: SourceSpan,
    },

    #[error("Expected identifier (no quotes needed)")]
    #[diagnostic(
        code(S23),
        url("https://docs.hw-script.org/errors/S23"),
        help("v0.1.6 uses bare identifiers for type names.\nExample: component Resistor: (not component \"Resistor\":)")
    )]
    ExpectedIdentifierNotString {
        #[label("Remove quotes")]
        span: SourceSpan,
    },

    #[error("The 'define' keyword was removed in v0.1.6")]
    #[diagnostic(
        code(S24),
        url("https://docs.hw-script.org/errors/S24"),
        help("Type keywords are now first-class. Use them directly.\nMigration: define component \"Name\": → component Name:")
    )]
    DefineKeywordRemoved {
        #[label("Remove 'define' keyword")]
        span: SourceSpan,
    },

    #[error("Register primitive is now lowercase")]
    #[diagnostic(
        code(S25),
        url("https://docs.hw-script.org/errors/S25"),
        help("v0.1.6 uses lowercase 'reg' for the register primitive.\nExample: reg(clock: Clk, reset: Rst, init: 0)")
    )]
    RegisterPrimitiveIsLowercase {
        #[label("Use 'reg' (not 'Reg')")]
        span: SourceSpan,
    },

    #[error("The 'fields:' keyword was removed")]
    #[diagnostic(
        code(S26),
        url("https://docs.hw-script.org/errors/S26"),
        help("v0.1.6 structs list fields directly without 'fields:' keyword.\nExample:\nstruct Instruction:\n    opcode[4]\n    operand[8]")
    )]
    FieldsKeywordRemoved {
        #[label("Remove 'fields:' keyword")]
        span: SourceSpan,
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

/// Create error for '=' found in property block (should be ':')
pub(crate) fn error_expected_colon_in_property(span: &Span) -> ParseError {
    ParseError::ExpectedColonInProperty {
        span: span_to_source_span(span),
    }
}

/// Create error for ':' found in logic block (should be '=')
#[allow(dead_code)] // Will be used when we detect ':' in logic blocks
pub(crate) fn error_expected_equals_in_logic(span: &Span) -> ParseError {
    ParseError::ExpectedEqualsInLogic {
        span: span_to_source_span(span),
    }
}

/// Create error for '==' found (should be single '=')
#[allow(dead_code)] // Will be used when we detect '==' operator
pub(crate) fn error_single_equals_for_comparison(span: &Span) -> ParseError {
    ParseError::UsesSingleEqualsForComparison {
        span: span_to_source_span(span),
    }
}

/// Create error for quoted identifier (should be bare identifier)
pub(crate) fn error_expected_identifier_not_string(span: &Span) -> ParseError {
    ParseError::ExpectedIdentifierNotString {
        span: span_to_source_span(span),
    }
}

/// Create error for 'define' keyword (removed in v0.1.6)
pub(crate) fn error_define_keyword_removed(span: &Span) -> ParseError {
    ParseError::DefineKeywordRemoved {
        span: span_to_source_span(span),
    }
}

/// Create error for uppercase 'Reg' (should be lowercase 'reg')
#[allow(dead_code)] // Will be used when we detect uppercase 'Reg'
pub(crate) fn error_register_primitive_is_lowercase(span: &Span) -> ParseError {
    ParseError::RegisterPrimitiveIsLowercase {
        span: span_to_source_span(span),
    }
}

/// Create error for 'fields:' keyword (removed in v0.1.6)
#[allow(dead_code)] // Will be used when we detect 'fields:' in struct definitions
pub(crate) fn error_fields_keyword_removed(span: &Span) -> ParseError {
    ParseError::FieldsKeywordRemoved {
        span: span_to_source_span(span),
    }
}

/// Create error for '%' used as binary operator (should use 'mod' keyword)
pub(crate) fn error_percent_as_operator(span: &Span) -> ParseError {
    ParseError::PercentAsOperator {
        span: span_to_source_span(span),
    }
}
