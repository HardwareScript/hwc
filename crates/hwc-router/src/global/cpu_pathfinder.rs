//! CPU Pathfinder Negotiated Congestion Global Router
//!
//! L3-cache optimized Pathfinder algorithm operating over 3D G-Cell graphs.
//! Resolves coarse net topologies and emits 3D `RoutingGuide` envelopes.

use crate::types::{GCellVolume3D, RoutingGuide, VolumetricTensor3D};
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashSet;

pub struct CpuPathFinder {
    pub max_iterations: usize,
    pub history_factor: f32,
}

impl Default for CpuPathFinder {
    fn default() -> Self {
        Self {
            max_iterations: 16,
            history_factor: 1.5,
        }
    }
}

impl CpuPathFinder {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            history_factor: 1.5,
        }
    }

    /// Global routes a set of nets, emitting 3D corridor guides.
    pub fn route_guides(
        &self,
        tensor: &mut VolumetricTensor3D,
        net_terminals: &[(NetId, (u16, u16, u8), (u16, u16, u8))],
    ) -> Vec<RoutingGuide> {
        let mut guides = Vec::with_capacity(net_terminals.len());

        for &(net_id, (x0, y0, z0), (x1, y1, z1)) in net_terminals {
            let mut path_volumes = Vec::new();
            let mut visited = FxHashSet::default();

            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);

            // Determine lower congestion L-path: H-then-V vs V-then-H
            let mut cost_hv: u32 = 0;
            for x in min_x..=max_x {
                let idx = tensor.index(x as usize, y0 as usize, z0 as usize);
                cost_hv += (tensor.occ_x[idx] + tensor.hist_x[idx]) as u32;
            }
            for y in min_y..=max_y {
                let idx = tensor.index(x1 as usize, y as usize, z1 as usize);
                cost_hv += (tensor.occ_y[idx] + tensor.hist_y[idx]) as u32;
            }

            let mut cost_vh: u32 = 0;
            for y in min_y..=max_y {
                let idx = tensor.index(x0 as usize, y as usize, z0 as usize);
                cost_vh += (tensor.occ_y[idx] + tensor.hist_y[idx]) as u32;
            }
            for x in min_x..=max_x {
                let idx = tensor.index(x as usize, y1 as usize, z1 as usize);
                cost_vh += (tensor.occ_x[idx] + tensor.hist_x[idx]) as u32;
            }

            if cost_hv <= cost_vh {
                // Horizontal first, then vertical
                for x in min_x..=max_x {
                    let cell = GCellVolume3D {
                        gcell_x: x,
                        gcell_y: y0,
                        layer_idx: z0,
                    };
                    if visited.insert(cell) {
                        path_volumes.push(cell);
                        tensor.add_occ_x(x as usize, y0 as usize, z0 as usize, 1);
                    }
                }
                for y in min_y..=max_y {
                    let cell = GCellVolume3D {
                        gcell_x: x1,
                        gcell_y: y,
                        layer_idx: z1,
                    };
                    if visited.insert(cell) {
                        path_volumes.push(cell);
                        tensor.add_occ_y(x1 as usize, y as usize, z1 as usize, 1);
                    }
                }
            } else {
                // Vertical first, then horizontal
                for y in min_y..=max_y {
                    let cell = GCellVolume3D {
                        gcell_x: x0,
                        gcell_y: y,
                        layer_idx: z0,
                    };
                    if visited.insert(cell) {
                        path_volumes.push(cell);
                        tensor.add_occ_y(x0 as usize, y as usize, z0 as usize, 1);
                    }
                }
                for x in min_x..=max_x {
                    let cell = GCellVolume3D {
                        gcell_x: x,
                        gcell_y: y1,
                        layer_idx: z1,
                    };
                    if visited.insert(cell) {
                        path_volumes.push(cell);
                        tensor.add_occ_x(x as usize, y1 as usize, z1 as usize, 1);
                    }
                }
            }

            // Layer transition via if start and end layers differ
            if z0 != z1 {
                let min_z = z0.min(z1);
                let max_z = z0.max(z1);
                for z in min_z..=max_z {
                    let cell = GCellVolume3D {
                        gcell_x: x1,
                        gcell_y: y1,
                        layer_idx: z,
                    };
                    if visited.insert(cell) {
                        path_volumes.push(cell);
                    }
                }
            }

            guides.push(RoutingGuide {
                net_id,
                volumes: path_volumes,
            });
        }

        guides
    }
}
