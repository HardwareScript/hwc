//! Chunk-level operations

use super::core::VoxelGrid;

impl VoxelGrid {
    /// Check if an entire chunk is empty (for A* router optimization).
    ///
    /// This enables the router to leap over 64-voxel regions in O(1) time.
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates (not voxel coordinates!)
    ///
    /// # Returns
    /// `true` if the entire 4x4x4 chunk is empty
    #[inline]
    pub fn is_chunk_empty(&self, chunk_x: usize, chunk_y: usize, chunk_z: usize) -> bool {
        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
        self.get_visible_chunk(chunk_index).is_none()
    }

    /// Convert voxel coordinates to chunk coordinates.
    ///
    /// # Returns
    /// (chunk_x, chunk_y, chunk_z)
    #[inline]
    pub fn voxel_to_chunk(x: usize, y: usize, z: usize) -> (usize, usize, usize) {
        (x >> 2, y >> 2, z >> 2)
    }

    /// Get the collision mask for a specific chunk.
    ///
    /// This enables cross-chunk collision detection by allowing the router
    /// to peek into neighboring chunks without falling back to slow voxel-by-voxel checks.
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates (not voxel coordinates!)
    ///
    /// # Returns
    /// The 64-bit collision mask where each bit represents one voxel in the 4×4×4 chunk.
    /// Returns 0 if the chunk is empty.
    #[inline]
    pub fn get_chunk_collision_mask(&self, chunk_x: usize, chunk_y: usize, chunk_z: usize) -> u64 {
        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
        self.get_visible_chunk(chunk_index)
            .map(|chunk| chunk.collision_mask)
            .unwrap_or(0)
    }

    /// Get the collision mask for a specific chunk by index.
    ///
    /// This is a lower-level method that skips coordinate-to-index conversion
    /// for performance-critical paths.
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_index` - Pre-computed chunk index
    ///
    /// # Returns
    /// The 64-bit collision mask, or 0 if the chunk is empty.
    #[inline]
    pub fn get_chunk_collision_mask_by_index(&self, chunk_index: usize) -> u64 {
        self.get_visible_chunk(chunk_index)
            .map(|chunk| chunk.collision_mask)
            .unwrap_or(0)
    }
}
