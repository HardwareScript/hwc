//! DOPHR: Data-Oriented Progressive Hierarchical Routing Engine
//!
//! Hardened 3-Stage Guided Routing Pipeline:
//! - Stage 1: 3D Volumetric Tensor Global Routing (PathFinder)
//! - Stage 2: Panel Track Assignment (Continuous Track Anchors)
//! - Stage 3: Guided Detailed Routing (Spatial 4-Color + Adaptive RRR)

use super::detailed::{DetailedSegment, DetailedTerminal, GuidedDetailedRouter};
use super::global::{GlobalTerminal, PathFinderGlobalRouter, RoutingGuide, VolumetricTensor3D};
use super::track_assign::{PanelTrackAssigner, TrackAnchor};
use hwc_types::NetId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for DOPHR synthesis pass
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DophrConfig {
    pub dim_x: usize,
    pub dim_y: usize,
    pub dim_z: usize,
    pub gcell_size_pm: i64,
    pub default_trace_width_pm: i64,
    pub drc_clearance_pm: i64,
    pub tracks_per_cell: u32,
    pub panel_size: u32,
    pub global_iterations: usize,
}

impl Default for DophrConfig {
    fn default() -> Self {
        Self {
            dim_x: 64,
            dim_y: 64,
            dim_z: 6,
            gcell_size_pm: 10_000_000,      // 10 um G-Cells
            default_trace_width_pm: 150_000, // 150 nm
            drc_clearance_pm: 150_000,       // 150 nm
            tracks_per_cell: 8,
            panel_size: 8,
            global_iterations: 15,
        }
    }
}

/// Final Output of the 3-Stage DOPHR Engine
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DophrRoutingResult {
    pub guides: HashMap<NetId, RoutingGuide>,
    pub anchors: HashMap<NetId, Vec<TrackAnchor>>,
    pub routed_segments: HashMap<NetId, Vec<DetailedSegment>>,
    pub total_wirelength_pm: i64,
}

/// Main DOPHR Engine Driver
pub struct DophrEngine {
    pub config: DophrConfig,
}

impl DophrEngine {
    pub fn new(config: DophrConfig) -> Self {
        Self { config }
    }

    /// Execute the complete 3-Stage Guided Routing gauntlet
    pub fn route_all(
        &self,
        net_terminals: &HashMap<NetId, Vec<DetailedTerminal>>,
    ) -> Result<DophrRoutingResult, String> {
        // -------------------------------------------------------------
        // STAGE 1: 3D Volumetric Tensor Global Routing
        // -------------------------------------------------------------
        let mut tensor = VolumetricTensor3D::new(
            self.config.dim_x,
            self.config.dim_y,
            self.config.dim_z,
            self.config.gcell_size_pm,
            self.config.tracks_per_cell as u16,
            self.config.tracks_per_cell as u16,
        );

        let mut global_terminals: HashMap<NetId, Vec<GlobalTerminal>> = HashMap::new();
        for (&net_id, terminals) in net_terminals {
            let mut g_terms = Vec::new();
            for t in terminals {
                let gx = ((t.x_pm / self.config.gcell_size_pm).max(0) as usize)
                    .min(self.config.dim_x.saturating_sub(1));
                let gy = ((t.y_pm / self.config.gcell_size_pm).max(0) as usize)
                    .min(self.config.dim_y.saturating_sub(1));
                let gz = (t.layer as usize).min(self.config.dim_z.saturating_sub(1));
                g_terms.push(GlobalTerminal { gx, gy, gz });
            }
            global_terminals.insert(net_id, g_terms);
        }

        let mut global_router =
            PathFinderGlobalRouter::new(&mut tensor, self.config.global_iterations);
        let mut guides = global_router.route_all(&global_terminals)?;

        // -------------------------------------------------------------
        // STAGE 2: Panel Track Assignment
        // -------------------------------------------------------------
        let track_assigner = PanelTrackAssigner::new(
            self.config.panel_size,
            self.config.gcell_size_pm,
            self.config.tracks_per_cell,
        );
        let anchors = track_assigner.assign_tracks(
            self.config.dim_x,
            self.config.dim_y,
            self.config.dim_z,
            &guides,
        );

        // -------------------------------------------------------------
        // STAGE 3: Guided Detailed Routing & Adaptive RRR
        // -------------------------------------------------------------
        let mut detailed_router = GuidedDetailedRouter::new(
            self.config.gcell_size_pm,
            self.config.default_trace_width_pm,
            self.config.drc_clearance_pm,
            self.config.dim_x as u32,
            self.config.dim_y as u32,
        );

        let mut routed_segments = HashMap::new();
        let mut total_wirelength_pm = 0i64;

        for (&net_id, terminals) in net_terminals {
            let empty_anchors = Vec::new();
            let net_anchors = anchors.get(&net_id).unwrap_or(&empty_anchors);

            if let Some(guide) = guides.get_mut(&net_id) {
                let segments = detailed_router
                    .route_net(net_id, terminals, net_anchors, guide)
                    .map_err(|e| format!("Detailed routing failed for net {:?}: {:?}", net_id, e))?;

                for seg in &segments {
                    let len = (seg.end_pm.0 - seg.start_pm.0).abs()
                        + (seg.end_pm.1 - seg.start_pm.1).abs();
                    total_wirelength_pm += len;
                }

                routed_segments.insert(net_id, segments);
            }
        }

        Ok(DophrRoutingResult {
            guides,
            anchors,
            routed_segments,
            total_wirelength_pm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dophr_end_to_end() {
        let config = DophrConfig {
            dim_x: 8,
            dim_y: 8,
            dim_z: 2,
            gcell_size_pm: 10_000_000,
            default_trace_width_pm: 150_000,
            drc_clearance_pm: 150_000,
            tracks_per_cell: 4,
            panel_size: 4,
            global_iterations: 5,
        };

        let engine = DophrEngine::new(config);
        let mut net_terminals = HashMap::new();

        let net_a = NetId::new(1);
        net_terminals.insert(
            net_a,
            vec![
                DetailedTerminal {
                    net_id: net_a,
                    layer: 0,
                    x_pm: 1_000_000,
                    y_pm: 1_000_000,
                },
                DetailedTerminal {
                    net_id: net_a,
                    layer: 0,
                    x_pm: 35_000_000,
                    y_pm: 35_000_000,
                },
            ],
        );

        let result = engine.route_all(&net_terminals).unwrap();
        assert!(result.routed_segments.contains_key(&net_a));
        assert!(result.total_wirelength_pm > 0);
    }
}
