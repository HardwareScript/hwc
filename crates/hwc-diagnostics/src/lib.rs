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

use compact_str::CompactString;
use miette::{Diagnostic, Report, Severity, SourceSpan};
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub mod location;
pub mod printer;

use printer::DiagnosticPrinter;

/// Error fingerprint for deduplication.
///
/// Groups errors by code and context to prevent spam from cascading errors.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ErrorFingerprint {
    /// Error code (e.g., "P16", "C14")
    pub code: CompactString,

    /// Context for grouping (e.g., "VCC_Core net", "Transistor_Q5")
    pub context: CompactString,
}

/// Central error accumulator for multi-error reporting.
///
/// Instead of returning `Result<T, E>` and stopping at the first error,
/// compilation passes report errors to this collector and continue.
///
/// This enables TypeScript-like multi-error reporting for large designs.
///
/// # Thread Safety
///
/// The collector uses `Arc<Mutex<>>` internally, making it safe to use
/// with parallel iterators (Rayon) for concurrent physics checking.
///
/// # Example
///
/// ```rust
/// use hwc_diagnostics::DiagnosticCollector;
///
/// let source = "...";
/// let collector = DiagnosticCollector::new(source, 20);
///
/// // Report errors without stopping (thread-safe)
/// // collector.report(some_error);
///
/// // Print all diagnostics at the end
/// collector.print_all();
///
/// if collector.has_errors() {
///     eprintln!("{}", collector.summary());
///     std::process::exit(1);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DiagnosticCollector {
    /// Thread-safe container for accumulated reports
    reports: Arc<Mutex<Vec<Report>>>,

    /// Track error occurrences for deduplication (using FxHashMap for performance)
    error_counts: Arc<Mutex<FxHashMap<ErrorFingerprint, usize>>>,

    /// Maximum errors before stopping (prevents infinite loops)
    pub max_errors: usize,

    /// Maximum identical errors to show (default: 3)
    pub max_duplicates: usize,

    /// Source code (for span extraction and error context)
    pub source: CompactString,
    
    /// File name/path of the source code
    pub file_name: CompactString,

    /// Sprint 9: Batch violations for pattern detection
    violations: Arc<Mutex<Vec<CollectedViolation>>>,
}

impl DiagnosticCollector {
    /// Create a new collector with source code, file name, and error limit.
    ///
    /// # Arguments
    ///
    /// * `source` - The source code being compiled (for span extraction)
    /// * `file_name` - The name or path of the source file
    /// * `max_errors` - Maximum number of errors before stopping (default: 20)
    pub fn new(source: &str, file_name: &str, max_errors: usize) -> Self {
        Self {
            reports: Arc::new(Mutex::new(Vec::new())),
            error_counts: Arc::new(Mutex::new(FxHashMap::default())),
            max_errors,
            max_duplicates: 3,
            source: source.into(),
            file_name: file_name.into(),
            violations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the maximum number of duplicate errors to show.
    ///
    /// # Example
    ///
    /// ```rust
    /// use hwc_diagnostics::DiagnosticCollector;
    /// let collector = DiagnosticCollector::new("source", 20)
    ///     .with_max_duplicates(5);
    /// ```
    pub fn with_max_duplicates(mut self, max: usize) -> Self {
        self.max_duplicates = max;
        self
    }

    /// Report an error or warning to the collector (thread-safe).
    ///
    /// This method accepts any type that implements `miette::Diagnostic`,
    /// which includes all Hardware Script error types (SymbolError,
    /// ValidationError, PhysicsError, etc.).
    pub fn report<E>(&self, error: E)
    where
        E: Diagnostic + Send + Sync + 'static,
    {
        // CRITICAL: Check if we've hit the limit to prevent memory explosion
        if self.should_stop() {
            return;
        }

        let mut reports = self.reports.lock().unwrap();
        // Attach source code to enable beautiful formatting with source snippets
        reports.push(Report::new(error).with_source_code(self.source.to_string()));
    }

    /// Sprint 9: Report a violation for pattern detection (Task 9.1/9.2)
    pub fn report_violation(&self, code: &str, message: &str, source_context: &str) {
        let mut violations = self.violations.lock().unwrap();
        violations.push(CollectedViolation {
            code: code.into(),
            message: message.into(),
            source_context: source_context.into(),
        });
    }

    /// Report an error with deduplication context (thread-safe).
    ///
    /// This method groups errors by code and context to prevent spam.
    /// If the same error occurs more than `max_duplicates` times,
    /// only the first few are shown.
    pub fn report_with_context<E>(&self, error: E, code: &str, context: &str)
    where
        E: Diagnostic + Send + Sync + 'static,
    {
        let fingerprint = ErrorFingerprint {
            code: code.into(),
            context: context.into(),
        };

        let mut counts = self.error_counts.lock().unwrap();
        let count = counts.entry(fingerprint.clone()).or_insert(0);
        *count += 1;

        // Only report if under duplicate limit
        if *count <= self.max_duplicates {
            drop(counts); // Release lock before acquiring reports lock
            let mut reports = self.reports.lock().unwrap();
            // Attach source code to enable beautiful formatting with source snippets
            // Convert CompactString to String for miette's SourceCode trait
            reports.push(Report::new(error).with_source_code(self.source.to_string()));
        }
    }

    /// Check if we should stop compilation (hit error limit).
    ///
    /// This prevents infinite loops when cascading errors occur.
    pub fn should_stop(&self) -> bool {
        self.error_count() >= self.max_errors
    }

    /// Check if any errors were reported (not just warnings) (thread-safe).
    ///
    /// Returns `true` if at least one error (not warning) was reported.
    pub fn has_errors(&self) -> bool {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .any(|r| r.severity().unwrap_or(Severity::Error) == Severity::Error)
    }

    /// Count only errors (not warnings) (thread-safe).
    pub fn error_count(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .filter(|r| r.severity().unwrap_or(Severity::Error) == Severity::Error)
            .count()
    }

    /// Count only warnings (thread-safe).
    pub fn warning_count(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .filter(|r| r.severity().unwrap_or(Severity::Error) == Severity::Warning)
            .count()
    }

    /// Count only advice/waivers (thread-safe).
    pub fn advice_count(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .filter(|r| r.severity().unwrap_or(Severity::Error) == Severity::Advice)
            .count()
    }

    /// Check if any diagnostics were reported (error, warning, or advice).
    pub fn has_any(&self) -> bool {
        let reports = self.reports.lock().unwrap();
        !reports.is_empty()
    }

    /// Print all accumulated diagnostics to stderr (thread-safe).
    ///
    /// This uses our custom Native Printer for full control over the output format.
    pub fn print_all(&self) {
        let reports = self.reports.lock().unwrap();
        let printer = DiagnosticPrinter::new(&self.source, &self.file_name);
        
        for report in reports.iter() {
            eprintln!("{}", printer.format_diagnostic(report.as_ref()));
        }

        // Sprint 9: Print Batch Validation report if violations exist
        self.print_violation_summary();
    }

    /// Sprint 9: Print pattern analysis and violation summary (Task 9.3)
    pub fn print_violation_summary(&self) {
        let violations = self.violations.lock().unwrap();
        if violations.is_empty() {
            return;
        }

        // Create a temporary ViolationCollector to reuse its logic
        let mut vc = ViolationCollector::new();
        for v in violations.iter() {
            vc.push(&v.code, &v.message, &v.source_context);
        }
        
        // Print the beautiful summary (Pattern Analysis, Stats, etc.)
        vc.print_report();
    }

    /// Print all diagnostics with deduplication summary (thread-safe).
    ///
    /// Shows hidden duplicate counts to prevent terminal spam.
    pub fn print_all_with_dedup(&self) {
        let reports = self.reports.lock().unwrap();
        let counts = self.error_counts.lock().unwrap();
        let printer = DiagnosticPrinter::new(&self.source, &self.file_name);

        // Print all unique errors with beautiful formatting
        for report in reports.iter() {
            eprintln!("{}", printer.format_diagnostic(report.as_ref()));
        }

        // Print deduplication summary
        let mut has_hidden = false;
        for (fingerprint, count) in counts.iter() {
            if *count > self.max_duplicates {
                if !has_hidden {
                    eprintln!("\n--- Deduplication Summary ---");
                    has_hidden = true;
                }
                let hidden = count - self.max_duplicates;
                eprintln!(
                    "⚠️  {} additional similar [{}] error{} on {} (hidden to reduce noise)",
                    hidden,
                    fingerprint.code,
                    if hidden == 1 { "" } else { "s" },
                    fingerprint.context
                );
            }
        }
    }

    /// Get a summary string (e.g., "3 errors, 2 warnings").
    pub fn summary(&self) -> CompactString {
        let errors = self.error_count();
        let warnings = self.warning_count();

        match (errors, warnings) {
            (0, 0) => "No errors or warnings".into(),
            (0, w) => format!("{} warning{}", w, if w == 1 { "" } else { "s" }).into(),
            (e, 0) => format!("{} error{}", e, if e == 1 { "" } else { "s" }).into(),
            (e, w) => format!(
                "{} error{}, {} warning{}",
                e,
                if e == 1 { "" } else { "s" },
                w,
                if w == 1 { "" } else { "s" }
            )
            .into(),
        }
    }

    /// Clear all accumulated diagnostics (thread-safe).
    pub fn clear(&self) {
        let mut reports = self.reports.lock().unwrap();
        reports.clear();
        let mut counts = self.error_counts.lock().unwrap();
        counts.clear();
    }

    /// Check if the collector is empty (no errors or warnings) (thread-safe).
    pub fn is_empty(&self) -> bool {
        let reports = self.reports.lock().unwrap();
        reports.is_empty()
    }

    /// Get the total number of diagnostics (errors + warnings) (thread-safe).
    pub fn len(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports.len()
    }

    /// Get the total number of errors including hidden duplicates (thread-safe).
    pub fn total_error_count(&self) -> usize {
        let counts = self.error_counts.lock().unwrap();
        counts.values().sum()
    }
}

impl Default for DiagnosticCollector {
    /// Create a collector with default settings (empty source, 20 error limit).
    fn default() -> Self {
        Self::new("", "unknown", 20)
    }
}

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

// ─────────────────────────────────────────────────────────────────────────────
// Sprint 9: Batch Validation — ViolationCollector
// ─────────────────────────────────────────────────────────────────────────────

/// A single collected violation with its violation type and source.
///
/// `source_context` is a human-readable label describing where the violation
/// originated, e.g. `"array 'Adder' at line 45"`.
#[derive(Debug, Clone)]
pub struct CollectedViolation {
    /// Short machine-readable type tag (e.g. `"P12"`, `"P44"`).
    pub code: CompactString,

    /// Human-readable description of the violation.
    pub message: CompactString,

    /// Source context: array name, loop variable, or placement identifier.
    pub source_context: CompactString,
}

/// Pattern detected in a batch of violations.
///
/// When many violations share the same `code` and `source_context` prefix
/// (e.g. all from the same array loop) they are grouped into a pattern so
/// the user sees one fix suggestion rather than hundreds of identical hints.
#[derive(Debug, Clone)]
pub struct ViolationPattern {
    /// Violation code shared by every instance in this pattern.
    pub code: CompactString,

    /// Human-readable description of the recurring pattern.
    pub description: CompactString,

    /// Total number of violations in this pattern.
    pub count: usize,

    /// The loop / array context this pattern was detected in.
    pub loop_context: CompactString,

    /// A single actionable fix suggestion for the entire pattern.
    pub suggested_fix: CompactString,
}

/// Accumulates violations during Phase 1 without aborting on the first error.
///
/// # Goals (Sprint 9)
/// - **Task 9.1**: Collect *all* violations before closing the Commit Gate.
/// - **Task 9.2**: Detect repeated violations in loops and group them.
/// - **Task 9.3**: Show first 10 in detail, summarise the rest.
///
/// # Usage
///
/// ```rust
/// use hwc_diagnostics::ViolationCollector;
///
/// let mut vc = ViolationCollector::new();
///
/// // During array unrolling — do NOT abort; just push:
/// vc.push("P12", "Instances 0 and 1 overlap", "array 'Adder'");
/// vc.push("P12", "Instances 1 and 2 overlap", "array 'Adder'");
///
/// // After placement phase:
/// if vc.has_violations() {
///     vc.print_report();
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct ViolationCollector {
    violations: Vec<CollectedViolation>,
}

impl ViolationCollector {
    /// Create an empty collector.
    pub fn new() -> Self {
        Self { violations: Vec::new() }
    }

    /// Push a new violation. Never aborts — always accumulates.
    pub fn push(&mut self, code: &str, message: &str, source_context: &str) {
        self.violations.push(CollectedViolation {
            code: code.into(),
            message: message.into(),
            source_context: source_context.into(),
        });
    }

    /// Returns `true` if at least one violation has been collected.
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }

    /// Total number of collected violations.
    pub fn len(&self) -> usize {
        self.violations.len()
    }

    /// Returns `true` if no violations have been collected.
    pub fn is_empty(&self) -> bool {
        self.violations.is_empty()
    }

    /// Drain all violations out of the collector (for conversion to errors).
    pub fn take_violations(&mut self) -> Vec<CollectedViolation> {
        std::mem::take(&mut self.violations)
    }

    /// Detect repeated patterns — violations that share the same code and
    /// source-context prefix (i.e. come from the same loop / array).
    ///
    /// Returns a list of `ViolationPattern` entries, one per detected group.
    pub fn detect_patterns(&self) -> Vec<ViolationPattern> {
        use rustc_hash::FxHashMap;

        // Group violations by (code, source_context)
        let mut groups: FxHashMap<(CompactString, CompactString), usize> =
            FxHashMap::default();

        for v in &self.violations {
            let key = (v.code.clone(), v.source_context.clone());
            let count = groups.entry(key).or_insert(0);
            *count += 1;
        }

        // Only flag as a "pattern" when the same (code, context) appears more
        // than once — meaning a loop is repeating the same mistake.
        let mut patterns: Vec<ViolationPattern> = Vec::new();
        for ((code, ctx), count) in groups {
            if count > 1 {
                // Find the first violation in this group to get its message
                let message = self.violations.iter()
                    .find(|v| v.code == code && v.source_context == ctx)
                    .map(|v| v.message.as_str())
                    .unwrap_or("repeated violation");

                let suggested_fix = match code.as_str() {
                    "P12" => format!(
                        "Increase pitch in {} so instances no longer overlap, \
                         or add `merge: [terminal]` if overlap is intentional.",
                        ctx
                    ),
                    "P44" => format!(
                        "Add `floating: true` waiver to every placement in {}, \
                         or connect components to the substrate surface.",
                        ctx
                    ),
                    "P42" => format!(
                        "Add `merge: true` waiver to {} if substrate embedding \
                         is intentional, or adjust Z-coordinates.",
                        ctx
                    ),
                    "P41" => format!(
                        "Add a via or trace to connect the isolated islands in {}.",
                        ctx
                    ),
                    _ => format!(
                        "Review all placements in {} and resolve the {} violation.",
                        ctx, code
                    ),
                };

                patterns.push(ViolationPattern {
                    code,
                    description: message.into(),
                    count,
                    loop_context: ctx,
                    suggested_fix: suggested_fix.into(),
                });
            }
        }

        // Deterministic ordering: most-frequent first
        patterns.sort_by(|a, b| b.count.cmp(&a.count));
        patterns
    }

    /// Print a full batch report to stderr.
    ///
    /// - Shows the first `MAX_SHOWN_VIOLATIONS` violations in detail.
    /// - Summarises the remainder.
    /// - Prints pattern-level fix suggestions when loops are detected.
    /// Sprint 9: Print pattern analysis as Rust-style notes (Task 9.3)
    pub fn print_report(&self) {
        if self.violations.is_empty() {
            return;
        }

        let patterns = self.detect_patterns();
        
        // Print patterns as Rust-style notes (like rustc)
        for p in patterns {
            if p.count > 1 {
                eprintln!(
                    "{} {} violations in array '{}' detected ({}: {})",
                    "note:".cyan().bold(),
                    p.count,
                    p.loop_context.as_str().bold(),
                    p.code,
                    p.description
                );
                eprintln!("      help: {}", p.suggested_fix);
            }
        }
    }

    /// Convert all collected violations into `IrError`-compatible strings
    /// for use at the Commit Gate.
    ///
    /// Returns `(Vec<String>, Vec<ViolationPattern>)` — the individual
    /// messages and the detected patterns.
    pub fn into_gate_report(self) -> (Vec<CollectedViolation>, Vec<ViolationPattern>) {
        let patterns = self.detect_patterns();
        (self.violations, patterns)
    }
}

// Implement Send and Sync for safe parallel usage
unsafe impl Send for DiagnosticCollector {}
unsafe impl Sync for DiagnosticCollector {}

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
