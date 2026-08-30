//! Stage 3: Panel Track Assignment (TA) & Pin Swapping
//!
//! Slices G-Cells into horizontal/vertical panels, solves Maximum Weight Bipartite
//! Matching for track placement, and executes dynamic NPN pin swapping.

pub mod bipartite;
pub mod pin_swap;

pub use bipartite::BipartiteTrackAssigner;
pub use pin_swap::{try_swap_symmetric_pins, InputSymmetryGroup};

use crate::traits::RoutingError;
use crate::types::{AssignedTrackSegment, RoutingGuide, VolumetricTensor3D};

pub struct TrackAssigner {
    bipartite_solver: BipartiteTrackAssigner,
}

impl Default for TrackAssigner {
    fn default() -> Self {
        Self {
            bipartite_solver: BipartiteTrackAssigner::default(),
        }
    }
}

impl TrackAssigner {
    pub fn new(track_pitch_pm: i64) -> Self {
        Self {
            bipartite_solver: BipartiteTrackAssigner::new(track_pitch_pm),
        }
    }

    /// Assigns tracks to routing guides.
    pub fn assign(
        &self,
        guides: &[RoutingGuide],
        tensor: &VolumetricTensor3D,
    ) -> Result<Vec<AssignedTrackSegment>, RoutingError> {
        Ok(self.bipartite_solver.assign_tracks(
            guides,
            tensor.gcell_width_pm,
            tensor.gcell_height_pm,
        ))
    }
}
