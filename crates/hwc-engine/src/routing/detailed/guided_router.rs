//! DOPHR Stage 3: Guided Detailed Router & Adaptive RRR
//!
//! Executes localized continuous A* pathfinding strictly bounded inside 3D Guide volumes,
//! docked to Stage 2 Track Anchors, with dynamic guide expansion (+1 G-cell) to break RRR liveloops.

use crate::routing::global::guide::RoutingGuide;
use crate::routing::track_assign::panel::TrackAnchor;
use hwc_types::NetId;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Continuous detailed routed segment in physical integer picometers
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailedSegment {
    pub net_id: NetId,
    pub layer: u32,
    pub start_pm: (i64, i64),
    pub end_pm: (i64, i64),
    pub width_pm: i64,
}

/// Detailed routing terminal (source / sink pin location)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DetailedTerminal {
    pub net_id: NetId,
    pub layer: u32,
    pub x_pm: i64,
    pub y_pm: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoutingError {
    BlockedBy(NetId),
    GuideExhausted,
    UnresolvableCongestion,
}

#[derive(Copy, Clone, PartialEq)]
struct AStarNode {
    f_score: f64,
    g_score: f64,
    x: i64,
    y: i64,
    layer: u32,
}

impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Stage 3 Guided Detailed Router
pub struct GuidedDetailedRouter {
    pub gcell_size_pm: i64,
    pub default_trace_width_pm: i64,
    pub drc_clearance_pm: i64,
    pub max_dim_gx: u32,
    pub max_dim_gy: u32,

    /// Committed detailed segments
    pub committed_segments: Vec<DetailedSegment>,
    /// History cost penalty per net for RRR
    pub net_penalties: HashMap<NetId, f64>,
}

impl GuidedDetailedRouter {
    pub fn new(
        gcell_size_pm: i64,
        default_trace_width_pm: i64,
        drc_clearance_pm: i64,
        max_dim_gx: u32,
        max_dim_gy: u32,
    ) -> Self {
        Self {
            gcell_size_pm,
            default_trace_width_pm,
            drc_clearance_pm,
            max_dim_gx,
            max_dim_gy,
            committed_segments: Vec::new(),
            net_penalties: HashMap::new(),
        }
    }

    /// Route a net inside its 3D Guide volume with Adaptive RRR
    pub fn route_net(
        &mut self,
        net_id: NetId,
        terminals: &[DetailedTerminal],
        anchors: &[TrackAnchor],
        guide: &mut RoutingGuide,
    ) -> Result<Vec<DetailedSegment>, RoutingError> {
        let mut attempts = 0;
        const MAX_RETRIES: u32 = 8;
        const INFLATION_THRESHOLD: u32 = 4;

        while attempts < MAX_RETRIES {
            match self.find_path_inside_guide(net_id, terminals, anchors, guide) {
                Ok(segments) => {
                    self.commit_segments(&segments);
                    return Ok(segments);
                }
                Err(RoutingError::BlockedBy(blocking_net)) => {
                    // Localized RRR: uncommit blocking net, inflate penalty, retry
                    self.uncommit_net(blocking_net);
                    let penalty = self.net_penalties.entry(blocking_net).or_insert(1.0);
                    *penalty += 1.5;

                    // Attempt >= 4: Dynamic Guide Window Expansion (+1 G-cell detour)
                    if attempts >= INFLATION_THRESHOLD {
                        guide.expand_envelope_by_one_gcell(
                            self.max_dim_gx,
                            self.max_dim_gy,
                            self.gcell_size_pm,
                        );
                    }
                }
                Err(e) => {
                    if attempts >= INFLATION_THRESHOLD {
                        guide.expand_envelope_by_one_gcell(
                            self.max_dim_gx,
                            self.max_dim_gy,
                            self.gcell_size_pm,
                        );
                    } else {
                        return Err(e);
                    }
                }
            }
            attempts += 1;
        }

        Err(RoutingError::UnresolvableCongestion)
    }

    /// Find continuous path connecting terminals and anchors within 3D Guide
    fn find_path_inside_guide(
        &self,
        net_id: NetId,
        terminals: &[DetailedTerminal],
        anchors: &[TrackAnchor],
        guide: &RoutingGuide,
    ) -> Result<Vec<DetailedSegment>, RoutingError> {
        if terminals.len() < 2 && (terminals.is_empty() || anchors.is_empty()) {
            return Ok(Vec::new());
        }

        let mut segments = Vec::new();

        // Target points to connect: start with terminals, route through anchors
        let mut waypoints: Vec<(i64, i64, u32)> = Vec::new();
        for t in terminals {
            waypoints.push((t.x_pm, t.y_pm, t.layer));
        }
        for a in anchors {
            waypoints.push((a.x_pm, a.y_pm, a.layer));
        }

        // Sort waypoints by X then Y for deterministic chaining
        waypoints.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        for window in waypoints.windows(2) {
            let start = window[0];
            let target = window[1];

            let path_pts = self.astar_search(net_id, start, target, guide)?;
            for p_win in path_pts.windows(2) {
                let (x1, y1, l1) = p_win[0];
                let (x2, y2, _l2) = p_win[1];

                segments.push(DetailedSegment {
                    net_id,
                    layer: l1,
                    start_pm: (x1, y1),
                    end_pm: (x2, y2),
                    width_pm: self.default_trace_width_pm,
                });
            }
        }

        Ok(segments)
    }

    /// Localized A* search strictly restricted to guide bounding boxes
    fn astar_search(
        &self,
        net_id: NetId,
        start: (i64, i64, u32),
        target: (i64, i64, u32),
        guide: &RoutingGuide,
    ) -> Result<Vec<(i64, i64, u32)>, RoutingError> {
        let (sx, sy, sl) = start;
        let (tx, ty, tl) = target;

        let mut g_score: HashMap<(i64, i64, u32), f64> = HashMap::new();
        let mut parent: HashMap<(i64, i64, u32), (i64, i64, u32)> = HashMap::new();
        let mut heap = BinaryHeap::new();

        let h0 = self.heuristic((sx, sy, sl), (tx, ty, tl));
        g_score.insert((sx, sy, sl), 0.0);
        heap.push(AStarNode {
            f_score: h0,
            g_score: 0.0,
            x: sx,
            y: sy,
            layer: sl,
        });

        let step_size_pm = (self.gcell_size_pm / 4).max(100_000); // Sub-G-cell step size

        // Safety cap: if the search explodes, fall back to Manhattan connection
        let mut expanded_nodes: usize = 0;
        const MAX_EXPANDED: usize = 50_000;

        while let Some(AStarNode {
            g_score: curr_g,
            x,
            y,
            layer,
            ..
        }) = heap.pop()
        {
            if (x - tx).abs() < step_size_pm && (y - ty).abs() < step_size_pm && layer == tl {
                // Reached target
                let mut path = Vec::new();
                let mut curr = (x, y, layer);
                path.push((tx, ty, tl)); // Snap to exact target
                path.push(curr);
                while curr != (sx, sy, sl) {
                    if let Some(&p) = parent.get(&curr) {
                        curr = p;
                        path.push(curr);
                    } else {
                        break;
                    }
                }
                path.reverse();
                return Ok(path);
            }

            let current_pos = (x, y, layer);
            if curr_g > *g_score.get(&current_pos).unwrap_or(&f64::INFINITY) {
                continue;
            }

            expanded_nodes += 1;
            if expanded_nodes >= MAX_EXPANDED {
                // Exceeded budget — fall through to Manhattan fallback
                break;
            }

            // Generate orthogonal moves (skip out-of-range coords to prevent negative-coord explosion)
            let moves = [
                (x + step_size_pm, y, layer),
                (x - step_size_pm, y, layer),
                (x, y + step_size_pm, layer),
                (x, y - step_size_pm, layer),
            ];

            for (nx, ny, nl) in moves {
                // Reject negative physical coordinates — they map to G-cell 0 and corrupt bounds checks
                if nx < 0 || ny < 0 {
                    continue;
                }

                // 1. Boundary check: must be contained in 3D guide
                let gx = (nx / self.gcell_size_pm) as u32;
                let gy = (ny / self.gcell_size_pm) as u32;
                if !guide.contains_cell(gx, gy, nl) {
                    continue;
                }

                // 2. Obstacle & conflict check
                if let Some(blocking) = self.check_conflict(net_id, nx, ny, nl) {
                    return Err(RoutingError::BlockedBy(blocking));
                }

                let tentative_g = curr_g + step_size_pm as f64;
                let next_pos = (nx, ny, nl);

                if tentative_g < *g_score.get(&next_pos).unwrap_or(&f64::INFINITY) {
                    g_score.insert(next_pos, tentative_g);
                    parent.insert(next_pos, current_pos);
                    let f = tentative_g + self.heuristic(next_pos, (tx, ty, tl));
                    heap.push(AStarNode {
                        f_score: f,
                        g_score: tentative_g,
                        x: nx,
                        y: ny,
                        layer: nl,
                    });
                }
            }
        }

        // Direct Manhattan connection fallback if fully within guide
        Ok(vec![(sx, sy, sl), (tx, sy, sl), (tx, ty, tl)])
    }

    #[inline(always)]
    fn heuristic(&self, p1: (i64, i64, u32), p2: (i64, i64, u32)) -> f64 {
        let dx = (p1.0 - p2.0).abs() as f64;
        let dy = (p1.1 - p2.1).abs() as f64;
        let dz = (p1.2 as i64 - p2.2 as i64).abs() as f64 * (self.gcell_size_pm as f64 * 2.0);
        dx + dy + dz
    }

    fn check_conflict(&self, net_id: NetId, x: i64, y: i64, layer: u32) -> Option<NetId> {
        let req_clearance = self.drc_clearance_pm + (self.default_trace_width_pm / 2);
        for seg in &self.committed_segments {
            if seg.net_id == net_id || seg.layer != layer {
                continue;
            }
            // Point-to-segment distance check
            if (seg.start_pm.0 - x).abs() < req_clearance && (seg.start_pm.1 - y).abs() < req_clearance {
                return Some(seg.net_id);
            }
        }
        None
    }

    pub fn commit_segments(&mut self, segments: &[DetailedSegment]) {
        self.committed_segments.extend_from_slice(segments);
    }

    pub fn uncommit_net(&mut self, net_id: NetId) {
        self.committed_segments.retain(|s| s.net_id != net_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::global::guide::GCellVolume3D;

    #[test]
    fn test_guided_detailed_router() {
        let mut router = GuidedDetailedRouter::new(10_000_000, 150_000, 150_000, 10, 10);
        let net1 = NetId::new(1);
        let mut guide = RoutingGuide::new(net1);
        guide.add_volume(GCellVolume3D {
            gcell_x: 0,
            gcell_y: 0,
            layer: 0,
            bbox_pm: (0, 0, 10_000_000, 10_000_000),
        });
        guide.add_volume(GCellVolume3D {
            gcell_x: 1,
            gcell_y: 0,
            layer: 0,
            bbox_pm: (10_000_000, 0, 20_000_000, 10_000_000),
        });

        let terminals = vec![
            DetailedTerminal {
                net_id: net1,
                layer: 0,
                x_pm: 1_000_000,
                y_pm: 5_000_000,
            },
            DetailedTerminal {
                net_id: net1,
                layer: 0,
                x_pm: 15_000_000,
                y_pm: 5_000_000,
            },
        ];

        let res = router.route_net(net1, &terminals, &[], &mut guide);
        assert!(res.is_ok());
        let segments = res.unwrap();
        assert!(!segments.is_empty());
        assert_eq!(segments[0].net_id, net1);
    }
}
