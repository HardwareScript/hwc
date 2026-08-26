//! DOPHR Stage 3: Spatial Graph 4-Coloring Dispatcher
//!
//! Groups G-cells into 4 non-adjacent spatial sets (RED, BLUE, GREEN, YELLOW)
//! so that concurrent worker threads can route cells in parallel with zero shared boundary contention.

use serde::{Deserialize, Serialize};

/// 4-Color partition set index
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSet {
    Red = 0,    // (even X, even Y)
    Blue = 1,   // (odd X, even Y)
    Green = 2,  // (even X, odd Y)
    Yellow = 3, // (odd X, odd Y)
}

impl ColorSet {
    pub fn all() -> [ColorSet; 4] {
        [
            ColorSet::Red,
            ColorSet::Blue,
            ColorSet::Green,
            ColorSet::Yellow,
        ]
    }

    #[inline(always)]
    pub fn from_gcell(gx: u32, gy: u32) -> Self {
        match (gx % 2, gy % 2) {
            (0, 0) => ColorSet::Red,
            (1, 0) => ColorSet::Blue,
            (0, 1) => ColorSet::Green,
            _ => ColorSet::Yellow,
        }
    }
}

/// G-Cell coordinate in spatial grid
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpatialCell {
    pub gx: u32,
    pub gy: u32,
    pub layer: u32,
}

/// Spatial 4-Color Scheduler
#[derive(Clone, Debug)]
pub struct ColorScheduler {
    pub dim_x: u32,
    pub dim_y: u32,
    pub dim_z: u32,
}

impl ColorScheduler {
    pub fn new(dim_x: u32, dim_y: u32, dim_z: u32) -> Self {
        Self {
            dim_x,
            dim_y,
            dim_z,
        }
    }

    /// Partition all active G-cells for a given layer into 4 colored batches
    pub fn partition_cells(&self, layer: u32) -> [Vec<SpatialCell>; 4] {
        let mut batches = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

        for gy in 0..self.dim_y {
            for gx in 0..self.dim_x {
                let color = ColorSet::from_gcell(gx, gy);
                batches[color as usize].push(SpatialCell { gx, gy, layer });
            }
        }

        batches
    }

    /// Check if two cells are strictly independent (no shared boundary or diagonal corner)
    pub fn are_cells_independent(c1: SpatialCell, c2: SpatialCell) -> bool {
        if c1.layer != c2.layer {
            return true;
        }
        let dx = (c1.gx as i64 - c2.gx as i64).abs();
        let dy = (c1.gy as i64 - c2.gy as i64).abs();
        dx > 1 || dy > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spatial_4_coloring() {
        let scheduler = ColorScheduler::new(4, 4, 1);
        let batches = scheduler.partition_cells(0);

        for (batch_idx, batch) in batches.iter().enumerate() {
            assert_eq!(batch.len(), 4);
            // Verify all cells in batch have the same color set
            for cell in batch {
                assert_eq!(ColorSet::from_gcell(cell.gx, cell.gy) as usize, batch_idx);
            }
        }
    }
}
