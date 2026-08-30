//! Tri-Hybrid Composite Physical Router Engine
//!
//! Orchestrates the multi-tier physical routing pipeline:
//! 1. Pin Access Analysis (PAA)
//! 2. 3D Volumetric Tensor Global Routing (FastGR GPU / CPU Pathfinder)
//! 3. Panel Track Assignment with NPN dynamic pin swapping
//! 4. Sparse-Grid Detailed Routing (Dr. CU 2.0)

use crate::detailed::DetailedRouter;
use crate::global::GlobalRouter;
use crate::paa::PinAccessAnalyzer;
use crate::track_assign::TrackAssigner;
use crate::traits::{RouterEngine, RoutingError, RoutingTask};
use crate::types::{RoutedOutput, VolumetricTensor3D};

pub struct TriHybridRouter {
    paa: PinAccessAnalyzer,
    global: GlobalRouter,
    track_assign: TrackAssigner,
    detailed: DetailedRouter,
}

impl Default for TriHybridRouter {
    fn default() -> Self {
        Self {
            paa: PinAccessAnalyzer::new(Default::default()),
            global: GlobalRouter::default(),
            track_assign: TrackAssigner::default(),
            detailed: DetailedRouter::default(),
        }
    }
}

impl TriHybridRouter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RouterEngine for TriHybridRouter {
    fn name(&self) -> &'static str {
        "HardwareScript Tri-Hybrid Physical Router (v0.3.1)"
    }

    fn route(&mut self, task: &RoutingTask) -> Result<RoutedOutput, RoutingError> {
        // 1. Pin Access Analysis (if not already computed)
        let pin_map = if task.pin_access_map.access_points.is_empty() {
            self.paa.analyze(task.entity_graph)?
        } else {
            task.pin_access_map.clone()
        };

        // 2. Global Routing with 14-byte SoA Volumetric Tensor
        let mut tensor = VolumetricTensor3D::new(64, 64, 5, 2_720_000, 2_720_000);
        let guides = self.global.route(task.entity_graph, &pin_map, &mut tensor)?;

        // 3. Panel Track Assignment
        let assigned_tracks = self.track_assign.assign(&guides, &tensor)?;

        // 4. Sparse-Grid Detailed Routing
        self.detailed.route(task.entity_graph, &pin_map, &assigned_tracks)
    }
}
