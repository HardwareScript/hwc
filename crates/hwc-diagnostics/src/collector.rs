use compact_str::CompactString;
use miette::{Report, Severity};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

use crate::printer::DiagnosticPrinter;
use crate::violations::{CollectedViolation, ViolationCollector};

/// Error fingerprint for deduplication.
///
/// Groups errors by code and context to prevent spam from cascading errors.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ErrorFingerprint {
    pub code: CompactString,
    pub context: CompactString,
}

/// Central error accumulator for multi-error reporting.
///
/// Instead of returning `Result<T, E>` and stopping at the first error,
/// compilation passes report errors to this collector and continue.
///
/// Thread-safe via `Arc<Mutex<>>`, safe for use with parallel iterators.
#[derive(Debug, Clone)]
pub struct DiagnosticCollector {
    reports: Arc<Mutex<Vec<Report>>>,
    error_counts: Arc<Mutex<FxHashMap<ErrorFingerprint, usize>>>,
    pub max_errors: usize,
    pub max_duplicates: usize,
    pub source: CompactString,
    pub file_name: CompactString,
    violations: Arc<Mutex<Vec<CollectedViolation>>>,
}

impl DiagnosticCollector {
    /// Create a new collector with source code and error limit.
    pub fn new(source: &str, max_errors: usize) -> Self {
        Self::new_with_file(source, "unknown", max_errors)
    }

    /// Create a new collector with source code, file name, and error limit.
    pub fn new_with_file(source: &str, file_name: &str, max_errors: usize) -> Self {
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
    pub fn with_max_duplicates(mut self, max: usize) -> Self {
        self.max_duplicates = max;
        self
    }

    /// Report an error or warning to the collector (thread-safe).
    pub fn report<E>(&self, error: E)
    where
        E: miette::Diagnostic + Send + Sync + 'static,
    {
        if self.should_stop() {
            return;
        }
        let mut reports = self.reports.lock().unwrap();
        reports.push(Report::new(error).with_source_code(self.source.to_string()));
    }

    /// Report a violation for pattern detection (Sprint 9).
    pub fn report_violation(&self, code: &str, message: &str, source_context: &str) {
        let mut violations = self.violations.lock().unwrap();
        violations.push(CollectedViolation {
            code: code.into(),
            message: message.into(),
            source_context: source_context.into(),
        });
    }

    /// Report an error with deduplication context (thread-safe).
    pub fn report_with_context<E>(&self, error: E, code: &str, context: &str)
    where
        E: miette::Diagnostic + Send + Sync + 'static,
    {
        let fingerprint = ErrorFingerprint {
            code: code.into(),
            context: context.into(),
        };

        let mut counts = self.error_counts.lock().unwrap();
        let count = counts.entry(fingerprint.clone()).or_insert(0);
        *count += 1;

        if *count <= self.max_duplicates {
            drop(counts);
            let mut reports = self.reports.lock().unwrap();
            reports.push(Report::new(error).with_source_code(self.source.to_string()));
        }
    }

    /// Check if we should stop compilation (hit error limit).
    pub fn should_stop(&self) -> bool {
        self.error_count() >= self.max_errors
    }

    /// Check if any errors were reported (not just warnings).
    pub fn has_errors(&self) -> bool {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .any(|r| r.severity().unwrap_or(Severity::Error) == Severity::Error)
    }

    /// Count only errors (not warnings).
    pub fn error_count(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .filter(|r| r.severity().unwrap_or(Severity::Error) == Severity::Error)
            .count()
    }

    /// Count only warnings.
    pub fn warning_count(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports
            .iter()
            .filter(|r| r.severity().unwrap_or(Severity::Error) == Severity::Warning)
            .count()
    }

    /// Count only advice/waivers.
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

    /// Print all accumulated diagnostics to stderr.
    pub fn print_all(&self) {
        let reports = self.reports.lock().unwrap();
        let printer = DiagnosticPrinter::new(&self.source, &self.file_name);
        for report in reports.iter() {
            eprintln!("{}", printer.format_diagnostic(report.as_ref()));
        }
        self.print_violation_summary();
    }

    /// Print pattern analysis and violation summary (Sprint 9).
    pub fn print_violation_summary(&self) {
        let violations = self.violations.lock().unwrap();
        if violations.is_empty() {
            return;
        }
        let mut vc = ViolationCollector::new();
        for v in violations.iter() {
            vc.push(&v.code, &v.message, &v.source_context);
        }
        vc.print_report();
    }

    /// Print all diagnostics with deduplication summary.
    pub fn print_all_with_dedup(&self) {
        let reports = self.reports.lock().unwrap();
        let counts = self.error_counts.lock().unwrap();
        let printer = DiagnosticPrinter::new(&self.source, &self.file_name);

        for report in reports.iter() {
            eprintln!("{}", printer.format_diagnostic(report.as_ref()));
        }

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

    /// Get formatted error messages as a string.
    pub fn format_errors(&self) -> String {
        let reports = self.reports.lock().unwrap();
        let printer = DiagnosticPrinter::new(&self.source, &self.file_name);
        reports
            .iter()
            .map(|report| printer.format_diagnostic(report.as_ref()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear all accumulated diagnostics.
    pub fn clear(&self) {
        let mut reports = self.reports.lock().unwrap();
        reports.clear();
        let mut counts = self.error_counts.lock().unwrap();
        counts.clear();
    }

    /// Check if the collector is empty.
    pub fn is_empty(&self) -> bool {
        let reports = self.reports.lock().unwrap();
        reports.is_empty()
    }

    /// Get the total number of diagnostics (errors + warnings).
    pub fn len(&self) -> usize {
        let reports = self.reports.lock().unwrap();
        reports.len()
    }

    /// Get the total number of errors including hidden duplicates.
    pub fn total_error_count(&self) -> usize {
        let counts = self.error_counts.lock().unwrap();
        counts.values().sum()
    }
}

impl Default for DiagnosticCollector {
    fn default() -> Self {
        Self::new("", 20)
    }
}

unsafe impl Send for DiagnosticCollector {}
unsafe impl Sync for DiagnosticCollector {}
