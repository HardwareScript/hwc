//! G-Cell-Local Unified Sweep Verification
//!
//! Complete G-cell-local DRC sweep engine with:
//! - Boundary-halo expansion for ghost segment detection
//! - Morton-ordered segment sorting for cache-friendly access
//! - Flat active-interval sweep (no BST, no pointer chasing)
//! - Unified overlap dispatch (same-net, different-net, no-overlap)
//! - std::thread::scope parallelism across G-cells

mod clearance;
mod ghost;
mod morton;
mod overlap;
mod sweep;
mod types;
mod verify;

pub use clearance::compute_actual_clearance;
pub use overlap::{classify_overlap, OverlapQuery, OverlapResult};
pub use sweep::{find_overlaps, segment_bbox, SegmentBbox};
pub use types::{BridgeTable, JunctionClassification, SweepViolation, ViolationType};
pub use verify::verify_gcell_sweep;
