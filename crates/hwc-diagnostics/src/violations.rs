use compact_str::CompactString;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;

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
#[derive(Debug, Default, Clone)]
pub struct ViolationCollector {
    violations: Vec<CollectedViolation>,
}

impl ViolationCollector {
    /// Create an empty collector.
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
        }
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
    pub fn detect_patterns(&self) -> Vec<ViolationPattern> {
        // Group violations by (code, source_context)
        let mut groups: FxHashMap<(CompactString, CompactString), usize> = FxHashMap::default();

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
                let message = self
                    .violations
                    .iter()
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

    /// Convert all collected violations into individual messages and patterns.
    pub fn into_gate_report(self) -> (Vec<CollectedViolation>, Vec<ViolationPattern>) {
        let patterns = self.detect_patterns();
        (self.violations, patterns)
    }
}
