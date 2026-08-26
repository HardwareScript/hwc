//! DOPHR Stage 2: Panel Track Assignment (Continuous Track Anchoring)
//!
//! Slices G-Cells into multi-cell panels, solves interval graph coloring for cross-panel trunks,
//! and assigns continuous physical track anchors to eliminate boundary deadlocks.

use crate::routing::global::guide::RoutingGuide;
use hwc_types::NetId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A fixed physical track anchor at a G-cell boundary (in integer picometers)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackAnchor {
    pub net_id: NetId,
    pub layer: u32,
    pub x_pm: i64,
    pub y_pm: i64,
    pub is_horizontal: bool,
    pub track_index: u32,
}

/// A panel of grouped G-cells (e.g. 8x1 horizontal or 1x8 vertical strip)
#[derive(Clone, Debug)]
pub struct Panel {
    pub panel_id: u32,
    pub layer: u32,
    pub is_horizontal: bool,
    pub start_gx: u32,
    pub start_gy: u32,
    pub length: u32,
    pub track_count: u32,
}

/// Interval representing a net trunk spanning across G-cells in a panel
#[derive(Clone, Debug)]
pub struct NetInterval {
    pub net_id: NetId,
    pub start_pos: u32,
    pub end_pos: u32,
}

/// Stage 2 Panel Track Assigner
pub struct PanelTrackAssigner {
    pub panel_size: u32,
    pub gcell_size_pm: i64,
    pub tracks_per_cell: u32,
}

impl PanelTrackAssigner {
    pub fn new(panel_size: u32, gcell_size_pm: i64, tracks_per_cell: u32) -> Self {
        Self {
            panel_size: panel_size.max(1),
            gcell_size_pm,
            tracks_per_cell,
        }
    }

    /// Assign track anchors for all nets across all panels
    pub fn assign_tracks(
        &self,
        dim_x: usize,
        dim_y: usize,
        dim_z: usize,
        guides: &HashMap<NetId, RoutingGuide>,
    ) -> HashMap<NetId, Vec<TrackAnchor>> {
        let mut anchors_by_net: HashMap<NetId, Vec<TrackAnchor>> = HashMap::new();
        let track_pitch_pm = self.gcell_size_pm / self.tracks_per_cell.max(1) as i64;

        // Process horizontal layers (e.g. even layers)
        for layer in 0..dim_z as u32 {
            let is_horizontal = layer % 2 == 0;

            if is_horizontal {
                // Horizontal panels: slice into rows of height 1 and width = panel_size
                for gy in 0..dim_y as u32 {
                    let mut gx = 0;
                    while gx < dim_x as u32 {
                        let span = (self.panel_size).min(dim_x as u32 - gx);
                        let intervals = self.extract_horizontal_intervals(gx, gy, span, layer, guides);
                        let assignments = self.color_intervals(&intervals, self.tracks_per_cell);

                        for (net_id, track_idx) in assignments {
                            // Compute Y coordinate of the track within this G-cell row
                            let y_base = gy as i64 * self.gcell_size_pm;
                            let y_pm = y_base + (track_idx as i64 * track_pitch_pm) + (track_pitch_pm / 2);

                            // Anchor at G-cell boundary crossings
                            for step in 0..=span {
                                let x_pm = (gx + step) as i64 * self.gcell_size_pm;
                                let anchor = TrackAnchor {
                                    net_id,
                                    layer,
                                    x_pm,
                                    y_pm,
                                    is_horizontal: true,
                                    track_index: track_idx,
                                };
                                anchors_by_net.entry(net_id).or_default().push(anchor);
                            }
                        }
                        gx += span;
                    }
                }
            } else {
                // Vertical panels: slice into columns of width 1 and height = panel_size
                for gx in 0..dim_x as u32 {
                    let mut gy = 0;
                    while gy < dim_y as u32 {
                        let span = (self.panel_size).min(dim_y as u32 - gy);
                        let intervals = self.extract_vertical_intervals(gx, gy, span, layer, guides);
                        let assignments = self.color_intervals(&intervals, self.tracks_per_cell);

                        for (net_id, track_idx) in assignments {
                            let x_base = gx as i64 * self.gcell_size_pm;
                            let x_pm = x_base + (track_idx as i64 * track_pitch_pm) + (track_pitch_pm / 2);

                            for step in 0..=span {
                                let y_pm = (gy + step) as i64 * self.gcell_size_pm;
                                let anchor = TrackAnchor {
                                    net_id,
                                    layer,
                                    x_pm,
                                    y_pm,
                                    is_horizontal: false,
                                    track_index: track_idx,
                                };
                                anchors_by_net.entry(net_id).or_default().push(anchor);
                            }
                        }
                        gy += span;
                    }
                }
            }
        }

        anchors_by_net
    }

    fn extract_horizontal_intervals(
        &self,
        start_gx: u32,
        gy: u32,
        span: u32,
        layer: u32,
        guides: &HashMap<NetId, RoutingGuide>,
    ) -> Vec<NetInterval> {
        let mut intervals = Vec::new();
        let end_gx = start_gx + span;

        for (&net_id, guide) in guides {
            let mut min_x = u32::MAX;
            let mut max_x = 0;
            let mut present = false;

            for v in &guide.volumes {
                if v.layer == layer && v.gcell_y == gy && v.gcell_x >= start_gx && v.gcell_x < end_gx {
                    present = true;
                    min_x = min_x.min(v.gcell_x);
                    max_x = max_x.max(v.gcell_x);
                }
            }

            if present {
                intervals.push(NetInterval {
                    net_id,
                    start_pos: min_x - start_gx,
                    end_pos: max_x - start_gx,
                });
            }
        }

        intervals
    }

    fn extract_vertical_intervals(
        &self,
        gx: u32,
        start_gy: u32,
        span: u32,
        layer: u32,
        guides: &HashMap<NetId, RoutingGuide>,
    ) -> Vec<NetInterval> {
        let mut intervals = Vec::new();
        let end_gy = start_gy + span;

        for (&net_id, guide) in guides {
            let mut min_y = u32::MAX;
            let mut max_y = 0;
            let mut present = false;

            for v in &guide.volumes {
                if v.layer == layer && v.gcell_x == gx && v.gcell_y >= start_gy && v.gcell_y < end_gy {
                    present = true;
                    min_y = min_y.min(v.gcell_y);
                    max_y = max_y.max(v.gcell_y);
                }
            }

            if present {
                intervals.push(NetInterval {
                    net_id,
                    start_pos: min_y - start_gy,
                    end_pos: max_y - start_gy,
                });
            }
        }

        intervals
    }

    /// Left-edge greedy interval graph coloring algorithm
    pub fn color_intervals(
        &self,
        intervals: &[NetInterval],
        max_tracks: u32,
    ) -> Vec<(NetId, u32)> {
        let mut sorted = intervals.to_vec();
        sorted.sort_by_key(|i| i.start_pos);

        let mut assignments = Vec::new();
        // track_end_pos[track_idx] records the rightmost position assigned so far
        let mut track_end_pos: Vec<u32> = Vec::new();

        for interval in sorted {
            let mut assigned_track = None;
            for (t_idx, &end) in track_end_pos.iter().enumerate() {
                if interval.start_pos > end {
                    assigned_track = Some(t_idx as u32);
                    break;
                }
            }

            let track = match assigned_track {
                Some(t) => {
                    track_end_pos[t as usize] = interval.end_pos;
                    t
                }
                None => {
                    let next_t = track_end_pos.len() as u32;
                    let bounded_t = next_t % max_tracks.max(1);
                    track_end_pos.push(interval.end_pos);
                    bounded_t
                }
            };

            assignments.push((interval.net_id, track));
        }

        assignments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::global::guide::GCellVolume3D;

    #[test]
    fn test_interval_graph_coloring() {
        let assigner = PanelTrackAssigner::new(8, 10_000_000, 4);
        let intervals = vec![
            NetInterval {
                net_id: NetId::new(1),
                start_pos: 0,
                end_pos: 3,
            },
            NetInterval {
                net_id: NetId::new(2),
                start_pos: 2,
                end_pos: 5,
            },
            NetInterval {
                net_id: NetId::new(3),
                start_pos: 4,
                end_pos: 7,
            },
        ];

        let assignments = assigner.color_intervals(&intervals, 4);
        assert_eq!(assignments.len(), 3);
        // Net 1 on track 0
        assert_eq!(assignments[0].1, 0);
        // Net 2 overlaps Net 1, so goes to track 1
        assert_eq!(assignments[1].1, 1);
        // Net 3 starts at 4 (> Net 1 end 3), can reuse track 0
        assert_eq!(assignments[2].1, 0);
    }
}
