//! Span utilities for converting parser spans to miette SourceSpans

use hwc_parser::Span;
use miette::SourceSpan;

/// Convert parser Span to miette SourceSpan
pub fn span_to_source_span(span: &Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), (span.end - span.start).into())
}

/// Create a default span for cases where we don't have span information
pub fn default_span() -> SourceSpan {
    SourceSpan::new(0.into(), 0.into())
}
