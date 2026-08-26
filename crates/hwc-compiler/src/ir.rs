//! Route Lockfile IR — v0.3.0 Stub
//!
//! The lockfile was a v0.2.x mechanism for caching routed nets between
//! builds. In v0.3.0, the comptime VM produces deterministic `EntityGraph`
//! geometry, making lockfiles largely redundant. These stubs preserve call
//! sites in `hwc-cli` while the v0.3.0 pipeline is being completed.

use hwc_engine::HardwareSpace;
use std::path::Path;

/// Persist validated route segments to a lockfile for deterministic re-builds.
///
/// **v0.3.0 status: Stub** — the v0.3.0 comptime evaluator emits
/// deterministic picometer geometry; route persistence will be revisited
/// once the full `evaluate_program` → EntityGraph pipeline is wired into
/// the build command.
pub fn save_routes_to_lockfile(_path: &Path, _space: &HardwareSpace, _source: &str) {
    eprintln!("[LOCK] save_routes_to_lockfile: not yet implemented in v0.3.0 pipeline.");
}
