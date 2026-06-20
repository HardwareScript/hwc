//! Memory statistics for voxel grid

use super::grid::VoxelGrid;

/// Memory usage statistics for a voxel grid.
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub total_voxels: usize,
    pub occupied_voxels: usize,
    pub occupancy_percent: f64,
    pub materials_bytes: usize,
    pub net_ids_bytes: usize,
    pub collision_bytes: usize,
    pub total_bytes: usize,
}

impl VoxelGrid {
    /// Get memory usage statistics.
    /// Thread-safe using safe helper methods.
    /// Accounts for both working and visible planes.
    pub fn memory_stats(&self) -> MemoryStats {
        let mut num_chunks = 0;
        let mut occupied_voxels = 0;

        // Count working plane (SPARSE: only iterate actual chunks)
        if let Ok(working_guard) = self.working_plane.read() {
            num_chunks += working_guard.len();
            for chunk in working_guard.values() {
                occupied_voxels += chunk.collision_mask.count_ones() as usize;
            }
        }

        // Count visible plane (SPARSE: only iterate actual chunks)
        if let Ok(visible_guard) = self.visible_plane.read() {
            num_chunks += visible_guard.len();
            for chunk in visible_guard.values() {
                occupied_voxels += chunk.collision_mask.count_ones() as usize;
            }
        }

        // Each chunk: 336 bytes (8 + 8 + 64 + 256)
        // HashMap overhead: minimal compared to dense Vec
        let directory_bytes = num_chunks * 64; // Approximate HashMap overhead per entry
        let chunk_bytes = num_chunks * 336;
        let total_bytes = directory_bytes + chunk_bytes;

        let occupancy_percent = if self.total_voxels > 0 {
            (occupied_voxels as f64 / self.total_voxels as f64) * 100.0
        } else {
            0.0
        };

        MemoryStats {
            total_voxels: self.total_voxels,
            occupied_voxels,
            occupancy_percent,
            materials_bytes: num_chunks * 64, // 64 bytes per chunk
            net_ids_bytes: num_chunks * 256,  // 256 bytes per chunk
            collision_bytes: num_chunks * 8,  // 8 bytes per chunk
            total_bytes,
        }
    }
}
