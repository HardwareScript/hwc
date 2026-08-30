//! 14-Byte SoA 3D Volumetric Capacity Tensor
//!
//! Compact Structure-of-Arrays (SoA) layout guaranteeing exactly 14 bytes per G-Cell,
//! designed to fit entirely in CPU L3 cache during Global Routing.

use crate::types::VolumetricTensor3D;

impl VolumetricTensor3D {
    /// Creates a new flat 14-byte/cell SoA volumetric tensor.
    pub fn new(
        dim_x: usize,
        dim_y: usize,
        dim_z: usize,
        gcell_width_pm: i64,
        gcell_height_pm: i64,
    ) -> Self {
        let total_cells = dim_x * dim_y * dim_z;
        Self {
            dim_x,
            dim_y,
            dim_z,
            gcell_width_pm: gcell_width_pm.max(1),
            gcell_height_pm: gcell_height_pm.max(1),
            cap_x: vec![10; total_cells],
            cap_y: vec![10; total_cells],
            occ_x: vec![0; total_cells],
            occ_y: vec![0; total_cells],
            hist_x: vec![0; total_cells],
            hist_y: vec![0; total_cells],
            base_cost: vec![1; total_cells],
        }
    }

    #[inline(always)]
    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dim_y + y) * self.dim_x + x
    }

    /// Returns a normalized congestion coefficient in [0.0, 1.0] for a given
    /// (x, y) picometer coordinate, for use as an analytical placement penalty weight.
    pub fn congestion_at_pm(&self, x_pm: i64, y_pm: i64) -> f32 {
        let gx = (x_pm.max(0) / self.gcell_width_pm) as usize;
        let gy = (y_pm.max(0) / self.gcell_height_pm) as usize;
        if gx >= self.dim_x || gy >= self.dim_y {
            return 0.0;
        }

        let mut max_cong: f32 = 0.0;
        for z in 0..self.dim_z {
            let idx = self.index(gx, gy, z);
            let cap = (self.cap_x[idx].max(self.cap_y[idx])) as f32;
            let occ = (self.occ_x[idx].max(self.occ_y[idx])) as f32;
            let cong = if cap == 0.0 { 1.0 } else { (occ / cap).min(1.0) };
            if cong > max_cong {
                max_cong = cong;
            }
        }
        max_cong
    }

    /// Add track occupancy to horizontal edge at (x, y, z).
    pub fn add_occ_x(&mut self, x: usize, y: usize, z: usize, delta: u16) {
        if x < self.dim_x && y < self.dim_y && z < self.dim_z {
            let idx = self.index(x, y, z);
            self.occ_x[idx] = self.occ_x[idx].saturating_add(delta);
        }
    }

    /// Add track occupancy to vertical edge at (x, y, z).
    pub fn add_occ_y(&mut self, x: usize, y: usize, z: usize, delta: u16) {
        if x < self.dim_x && y < self.dim_y && z < self.dim_z {
            let idx = self.index(x, y, z);
            self.occ_y[idx] = self.occ_y[idx].saturating_add(delta);
        }
    }

    /// Checks if a cell is congested on layer z.
    pub fn is_congested(&self, x: usize, y: usize, z: usize) -> bool {
        if x >= self.dim_x || y >= self.dim_y || z >= self.dim_z {
            return true;
        }
        let idx = self.index(x, y, z);
        self.occ_x[idx] > self.cap_x[idx] || self.occ_y[idx] > self.cap_y[idx]
    }
}
