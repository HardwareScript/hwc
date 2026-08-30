//! Stage 2: 3D Volumetric Capacity Tensor & Global Routing
//!
//! Evaluates coarse G-Cell capacity, models via porosity, solves FastGR GPU
//! pattern routing or CPU Pathfinder, and emits 3D `RoutingGuide` volumes.

pub mod cpu_pathfinder;
pub mod cuda_fastgr;
pub mod tensor;

pub use cpu_pathfinder::CpuPathFinder;
pub use cuda_fastgr::CudaFastGr;

use crate::traits::RoutingError;
use crate::types::{PinAccessMap, RoutingGuide, VolumetricTensor3D};
use hwc_engine::netlist::NetId;
use hwc_engine::EntityGraph;
use rustc_hash::FxHashMap;

/// Stage 2 Global Routing Coordinator
pub struct GlobalRouter {
    cpu_pathfinder: CpuPathFinder,
    cuda_fastgr: CudaFastGr,
}

impl Default for GlobalRouter {
    fn default() -> Self {
        Self {
            cpu_pathfinder: CpuPathFinder::default(),
            cuda_fastgr: CudaFastGr::default(),
        }
    }
}

impl GlobalRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Executes global routing across all nets in the entity graph.
    pub fn route(
        &self,
        entity_graph: &EntityGraph,
        _pin_map: &PinAccessMap,
        tensor: &mut VolumetricTensor3D,
    ) -> Result<Vec<RoutingGuide>, RoutingError> {
        let mut terminals = Vec::new();
        let mut net_pins: FxHashMap<NetId, Vec<(u16, u16, u8)>> = FxHashMap::default();

        // Group component pins by net
        for (i, pin) in entity_graph.get_component_pins().iter().enumerate() {
            let net_id = NetId::new((i / 2) as u32);
            let gx = (((pin.x_nm * 1000).max(0) / tensor.gcell_width_pm) as u16)
                .min((tensor.dim_x - 1) as u16);
            let gy = (((pin.y_nm * 1000).max(0) / tensor.gcell_height_pm) as u16)
                .min((tensor.dim_y - 1) as u16);
            net_pins.entry(net_id).or_default().push((gx, gy, 0));
        }

        // Create 2-pin decomposition pairs for global routing
        for (net_id, pts) in net_pins {
            if pts.len() >= 2 {
                terminals.push((net_id, pts[0], pts[1]));
            }
        }

        // Try GPU FastGR first, otherwise fall back to CPU Pathfinder
        if let Some(guides) = self.cuda_fastgr.route_pattern(tensor, &terminals) {
            Ok(guides)
        } else {
            Ok(self.cpu_pathfinder.route_guides(tensor, &terminals))
        }
    }
}
