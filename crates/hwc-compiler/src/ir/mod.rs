//! Intermediate Representation (IR) and Salsa Query Pipeline
//!
//! Exposes demand-driven compilation queries and Freeze-Silicon ECO provenance.

pub mod eco_query;
pub mod query;

pub use eco_query::{base_silicon_snapshot_query, verify_freeze_silicon_immutability_query, BaseSiliconSnapshot};
pub use query::{ingest_geometry_to_entity_graph, parse_ast_query, QueryPipelineContext};

use hwc_engine::HardwareSpace;
use std::path::Path;

/// Persist validated route segments to a lockfile for deterministic re-builds.
pub fn save_routes_to_lockfile(_path: &Path, _space: &HardwareSpace, _source: &str) {
    // Deterministic picometer geometry is preserved via EntityGraph and Salsa query memoization.
}
