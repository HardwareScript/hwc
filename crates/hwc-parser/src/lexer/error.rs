//! Lexer error types with miette diagnostics

use super::span::Span;
use compact_str::CompactString;

/// Lexer errors with authoritative, professional miette diagnostics
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum LexError {
    #[error("Invalid character '{text}'")]
    #[diagnostic(
        code(S11),
        url("https://docs.hw-script.org/errors/S11"),
        help("Character '{text}' is not valid in HardwareScript v0.3.0 syntax. Check for typos or unexpected symbols.")
    )]
    InvalidToken {
        #[label("Invalid character here")]
        span: miette::SourceSpan,
        text: CompactString,
    },
}

/// Convert our Span to miette's SourceSpan
pub fn span_to_source_span(span: &Span) -> miette::SourceSpan {
    (span.start, span.end - span.start).into()
}
