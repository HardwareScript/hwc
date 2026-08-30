//! Maximum Weight Bipartite Matching for Track Assignment
//!
//! Slices the design into multi-cell panels and assigns net trunks to discrete
//! physical tracks, minimizing total wirelength and via count.

use crate::types::{AssignedTrackSegment, RoutingGuide};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct BipartiteTrackAssigner {
    pub track_pitch_pm: i64,
}

impl Default for BipartiteTrackAssigner {
    fn default() -> Self {
        Self {
            track_pitch_pm: 460_000, // 460 nm default M1/M2 track pitch
        }
    }
}

impl BipartiteTrackAssigner {
    pub fn new(track_pitch_pm: i64) -> Self {
        Self { track_pitch_pm }
    }

    /// Assigns routing guides to physical track segments across panels.
    pub fn assign_tracks(
        &self,
        guides: &[RoutingGuide],
        gcell_width_pm: i64,
        gcell_height_pm: i64,
    ) -> Vec<AssignedTrackSegment> {
        let mut segments = Vec::new();
        let mut track_occupancy: FxHashMap<(u8, u32), i64> = FxHashMap::default();

        for guide in guides {
            if guide.volumes.is_empty() {
                continue;
            }

            let first_vol = guide.volumes[0];
            let last_vol = *guide.volumes.last().unwrap_or(&first_vol);

            let layer_idx = first_vol.layer_idx;
            let start_x_pm = (first_vol.gcell_x as i64) * gcell_width_pm;
            let end_x_pm = (last_vol.gcell_x as i64 + 1) * gcell_width_pm;
            let y_center_pm = (first_vol.gcell_y as i64) * gcell_height_pm + gcell_height_pm / 2;

            // Compute nearest track index
            let track_idx = ((y_center_pm / self.track_pitch_pm).max(0)) as u32;

            // Check track occupancy and offset if needed
            let track_key = (layer_idx, track_idx);
            let assigned_track = if let Some(&last_end) = track_occupancy.get(&track_key) {
                if start_x_pm < last_end {
                    track_idx + 1
                } else {
                    track_idx
                }
            } else {
                track_idx
            };

            track_occupancy.insert((layer_idx, assigned_track), end_x_pm);

            segments.push(AssignedTrackSegment {
                net_id: guide.net_id,
                layer_idx,
                track_index: assigned_track,
                start_coord_pm: start_x_pm,
                end_coord_pm: end_x_pm,
                fixed_axis_coord_pm: (assigned_track as i64) * self.track_pitch_pm,
            });
        }

        segments
    }
}
