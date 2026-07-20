//! Diagnostic Collector for Multi-Error Reporting
//!
//! This crate provides the shared diagnostic infrastructure for the Hardware Script compiler.
//! It implements the "Error Recovery" strategy, enabling TypeScript-like multi-error reporting
//! for large-scale SoC designs.
//!
//! # Architecture
//!
//! This is a leaf crate in the compiler architecture:
//! - `hwc-diagnostics` (this crate) - Depends only on `miette`
//! - `hwc-parser` - Depends on `hwc-diagnostics`
//! - `hwc-compiler` - Depends on `hwc-parser` and `hwc-diagnostics`
//!
//! This design breaks the circular dependency and creates a clean DAG.

mod collector;
mod violations;

pub mod location;
pub mod printer;

pub use collector::{DiagnosticCollector, ErrorFingerprint};
pub use violations::{CollectedViolation, ViolationCollector, ViolationPattern};

use compact_str::CompactString;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Diagnostic for applied design waivers (Silicon Law)
///
/// v0.1.7: Replaces println! logs with formal diagnostics for the Native Printer.
#[derive(Debug, Error, Diagnostic)]
#[error("Waiver applied: {message}")]
#[diagnostic(code("W001"), severity(Advice))]
pub struct WaiverApplied {
    pub message: CompactString,

    #[label("this intentional deviation was permitted")]
    pub span: Option<SourceSpan>,
}

impl WaiverApplied {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn with_span(message: &str, start: usize, len: usize) -> Self {
        Self {
            message: message.into(),
            span: Some(SourceSpan::new(start.into(), len.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miette::Diagnostic;
    use thiserror::Error;

    #[derive(Error, Debug, Diagnostic)]
    #[error("Test error")]
    struct TestError;

    #[derive(Error, Debug, Diagnostic)]
    #[error("Test warning")]
    #[diagnostic(severity(Warning))]
    struct TestWarning;

    #[test]
    fn test_new_collector() {
        let collector = DiagnosticCollector::new("source code", 10);
        assert_eq!(collector.max_errors, 10);
        assert_eq!(collector.source, "source code");
        assert!(collector.is_empty());
    }

    #[test]
    fn test_report_error() {
        let collector = DiagnosticCollector::new("", 10);
        collector.report(TestError);
        assert_eq!(collector.error_count(), 1);
        assert_eq!(collector.warning_count(), 0);
        assert!(collector.has_errors());
    }

    #[test]
    fn test_report_warning() {
        let collector = DiagnosticCollector::new("", 10);
        collector.report(TestWarning);
        assert_eq!(collector.error_count(), 0);
        assert_eq!(collector.warning_count(), 1);
        assert!(!collector.has_errors());
    }

    #[test]
    fn test_should_stop() {
        let collector = DiagnosticCollector::new("", 3);
        assert!(!collector.should_stop());

        collector.report(TestError);
        assert!(!collector.should_stop());

        collector.report(TestError);
        assert!(!collector.should_stop());

        collector.report(TestError);
        assert!(collector.should_stop());
    }

    #[test]
    fn test_summary() {
        let collector = DiagnosticCollector::new("", 10);
        assert_eq!(collector.summary(), "No errors or warnings");

        collector.report(TestError);
        assert_eq!(collector.summary(), "1 error");

        collector.report(TestError);
        assert_eq!(collector.summary(), "2 errors");

        collector.report(TestWarning);
        assert_eq!(collector.summary(), "2 errors, 1 warning");

        collector.report(TestWarning);
        assert_eq!(collector.summary(), "2 errors, 2 warnings");
    }

    #[test]
    fn test_clear() {
        let collector = DiagnosticCollector::new("", 10);
        collector.report(TestError);
        collector.report(TestWarning);
        assert_eq!(collector.len(), 2);

        collector.clear();
        assert!(collector.is_empty());
        assert_eq!(collector.error_count(), 0);
        assert_eq!(collector.warning_count(), 0);
    }

    #[test]
    fn test_default() {
        let collector = DiagnosticCollector::default();
        assert_eq!(collector.max_errors, 20);
        assert_eq!(collector.source, "");
        assert!(collector.is_empty());
    }

    #[test]
    fn test_deduplication() {
        let collector = DiagnosticCollector::new("", 10).with_max_duplicates(2);

        // Report same error 5 times
        for _ in 0..5 {
            collector.report_with_context(TestError, "P16", "VCC_Core");
        }

        // Should only show 2 (max_duplicates)
        assert_eq!(collector.len(), 2);

        // But total count should be 5
        assert_eq!(collector.total_error_count(), 5);
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let collector = DiagnosticCollector::new("", 100);
        let collector_clone = collector.clone();

        // Spawn thread that reports errors
        let handle = thread::spawn(move || {
            for _ in 0..10 {
                collector_clone.report(TestError);
            }
        });

        // Report errors from main thread
        for _ in 0..10 {
            collector.report(TestError);
        }

        handle.join().unwrap();

        // Should have 20 errors total
        assert_eq!(collector.error_count(), 20);
    }
}
