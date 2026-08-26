//! DOPHR Stage 1: 3D Routing Guide Volumes
//!
//! Output by Stage 1 Global Router to bound detailed routing within discrete 3D spatial boxes.

use hwc_types::NetId;
use serde::{Deserialize, Serialize};

/// Discrete 3D G-Cell spatial volume assigned to a net's global route
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GCellVolume3D {
    pub gcell_x: u32,
    pub gcell_y: u32,
    pub layer: u32,
    /// Bounding box in integer picometers (min_x, min_y, max_x, max_y)
    pub bbox_pm: (i64, i64, i64, i64),
}

/// 3D Routing Guide emitted by Stage 1 Global Router
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingGuide {
    pub net_id: NetId,
    pub volumes: Vec<GCellVolume3D>,
}

impl RoutingGuide {
    pub fn new(net_id: NetId) -> Self {
        Self {
            net_id,
            volumes: Vec::new(),
        }
    }

    pub fn with_volumes(net_id: NetId, volumes: Vec<GCellVolume3D>) -> Self {
        Self { net_id, volumes }
    }

    pub fn add_volume(&mut self, volume: GCellVolume3D) {
        if !self.volumes.contains(&volume) {
            self.volumes.push(volume);
        }
    }

    /// Check if a given (x, y, layer) in G-cell coordinates is covered by this guide
    pub fn contains_cell(&self, gx: u32, gy: u32, layer: u32) -> bool {
        self.volumes
            .iter()
            .any(|v| v.gcell_x == gx && v.gcell_y == gy && v.layer == layer)
    }

    /// Adaptive Guide Inflation (+1 G-Cell detour window)
    /// Dynamically expands the guide boundary in all planar directions to break localized RRR liveloops.
    pub fn expand_envelope_by_one_gcell(
        &mut self,
        max_gx: u32,
        max_gy: u32,
        gcell_size_pm: i64,
    ) {
        let mut new_volumes = self.volumes.clone();
        for v in &self.volumes {
            let gx = v.gcell_x;
            let gy = v.gcell_y;
            let layer = v.layer;

            let neighbors = [
                (gx.saturating_sub(1), gy),
                ((gx + 1).min(max_gx.saturating_sub(1)), gy),
                (gx, gy.saturating_sub(1)),
                (gx, (gy + 1).min(max_gy.saturating_sub(1))),
            ];

            for (nx, ny) in neighbors {
                let min_x = nx as i64 * gcell_size_pm;
                let min_y = ny as i64 * gcell_size_pm;
                let max_x = min_x + gcell_size_pm;
                let max_y = min_y + gcell_size_pm;

                let nv = GCellVolume3D {
                    gcell_x: nx,
                    gcell_y: ny,
                    layer,
                    bbox_pm: (min_x, min_y, max_x, max_y),
                };
                if !new_volumes.contains(&nv) {
                    new_volumes.push(nv);
                }
            }
        }
        self.volumes = new_volumes;
    }
}
