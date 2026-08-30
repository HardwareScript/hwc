//! Stage 4: Sparse-Grid Detailed Routing
//!
//! Multi-level sparse grid graph A* search bounded by 3D guides with in-search
//! lookahead DRC and timing-slack-weighted negotiated congestion rip-up & repair.

pub mod lookahead_drc;
pub mod sparse_grid;
pub mod timing_rrr;

pub use lookahead_drc::{validate_wire_segment, DrcRules};
pub use sparse_grid::SparseGridDetailedRouter;
pub use timing_rrr::TimingSlackMap;

use crate::traits::RoutingError;
use crate::types::{AssignedTrackSegment, PinAccessMap, RoutedOutput};
use hwc_engine::EntityGraph;

pub struct DetailedRouter {
    sparse_router: SparseGridDetailedRouter,
}

impl Default for DetailedRouter {
    fn default() -> Self {
        Self {
            sparse_router: SparseGridDetailedRouter::default(),
        }
    }
}

impl DetailedRouter {
    pub fn new(rules: DrcRules) -> Self {
        Self {
            sparse_router: SparseGridDetailedRouter::new(rules),
        }
    }

    /// Executes detailed routing over assigned tracks.
    pub fn route(
        &self,
        entity_graph: &EntityGraph,
        pin_map: &PinAccessMap,
        assigned_tracks: &[AssignedTrackSegment],
    ) -> Result<RoutedOutput, RoutingError> {
        Ok(self
            .sparse_router
            .route_detailed(entity_graph, pin_map, assigned_tracks))
    }
}
