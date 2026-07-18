/// Build configuration options
use compact_str::CompactString;

pub struct BuildConfig {
    pub skip_drc: bool,
    pub skip_physics: bool,
    pub skip_connectivity_check: bool,
    #[allow(dead_code)]
    pub skip_alignment: bool, // Sprint 4.1: Skip Alignment Layer validation
    pub skip_physical_continuity: bool, // Task 4.3: Skip physical continuity check (P41/P42/P43)
    pub skip_bulk_validation: bool,     // Task 4.3: Skip bulk connection validation
    pub no_lockfile: bool,
    pub force_reroute: bool,
    pub force_export: bool, // Task 5.3: Override Commit Gate for debugging
    pub verbose: bool,
    pub limit: Option<usize>,
    pub all: bool,
    pub deny_warnings: bool,
    pub space: Option<CompactString>, // Filter to build only a specific space
    pub tolerance: Option<f64>,       // Alignment validation tolerance (default: 0.01 = 1%)
    pub debug_identity: bool, // v0.1.7: Trace net decomposition (LogicalNet → Route → Physical)
    pub verify_only: bool,    // v0.1.7: Run verification without export
}
