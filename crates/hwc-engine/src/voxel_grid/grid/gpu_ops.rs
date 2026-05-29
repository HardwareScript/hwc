//! GPU buffer and rendering operations

use super::core::VoxelGrid;
use crate::voxel_grid::shared_buffer::SharedVoxelBuffer;
use std::sync::Arc;

impl VoxelGrid {
    /// Get a pointer to the GPU-compatible buffer for a specific chunk.
    ///
    /// This enables zero-copy GPU rendering by allowing the GPU to read
    /// directly from compiler memory without CPU→GPU transfers.
    ///
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_index` - Pre-computed chunk index
    ///
    /// # Returns
    /// Pointer to chunk data (16-byte aligned) or null if chunk is empty.
    /// The pointer is opaque - GPU shaders interpret the memory layout.
    ///
    /// # Safety
    /// The returned pointer is valid as long as:
    /// - The VoxelGrid is not dropped
    /// - commit_route() is not called (which may free the chunk)
    /// - The chunk is not modified
    ///
    /// GPU shaders should check for null pointers before dereferencing.
    ///
    /// # Example
    /// ```ignore
    /// let chunk_ptr = grid.get_gpu_buffer_ptr(chunk_index);
    /// if !chunk_ptr.is_null() {
    ///     // Pass pointer to GPU shader
    ///     // Shader can read collision_mask, materials, handles directly
    /// }
    /// ```
    #[inline]
    pub fn get_gpu_buffer_ptr(&self, chunk_index: usize) -> *const u8 {
        self.get_visible_chunk(chunk_index)
            .map(|arc| Arc::as_ptr(&arc) as *const u8)
            .unwrap_or(std::ptr::null())
    }

    /// Get GPU buffer pointers for all chunks in a region.
    ///
    /// This is optimized for viewport rendering where the GPU needs to
    /// render a specific region of the design.
    ///
    /// # Arguments
    /// * `min` - Minimum corner (x, y, z) in voxels
    /// * `max` - Maximum corner (x, y, z) in voxels
    ///
    /// # Returns
    /// Vec of (chunk_index, chunk_ptr) pairs for non-empty chunks in the region.
    /// Pointers are 16-byte aligned and GPU-compatible.
    pub fn get_gpu_buffer_ptrs_in_region(
        &self,
        min: (usize, usize, usize),
        max: (usize, usize, usize),
    ) -> Vec<(usize, *const u8)> {
        let (min_x, min_y, min_z) = min;
        let (max_x, max_y, max_z) = max;

        // Convert to chunk coordinates
        let (min_chunk_x, min_chunk_y, min_chunk_z) = Self::voxel_to_chunk(min_x, min_y, min_z);
        let (max_chunk_x, max_chunk_y, max_chunk_z) = Self::voxel_to_chunk(max_x, max_y, max_z);

        // Get chunk dimensions to avoid out of bounds
        let (chunks_x, chunks_y, chunks_z) = self.chunk_dimensions();

        let mut result = Vec::new();

        // Scan all chunks in the bounding box
        for chunk_z in min_chunk_z..=max_chunk_z.min(chunks_z.saturating_sub(1)) {
            for chunk_y in min_chunk_y..=max_chunk_y.min(chunks_y.saturating_sub(1)) {
                for chunk_x in min_chunk_x..=max_chunk_x.min(chunks_x.saturating_sub(1)) {
                    let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);

                    if let Some(chunk) = self.get_visible_chunk(chunk_index) {
                        result.push((chunk_index, Arc::as_ptr(&chunk) as *const u8));
                    }
                }
            }
        }

        result
    }

    /// Get the list of dirty chunk indices for incremental GPU updates.
    ///
    /// This enables the GPU to only re-render chunks that have changed,
    /// dramatically improving viewport performance for large designs.
    ///
    /// # Returns
    /// Vec of chunk indices that have been modified since last clear.
    pub fn get_dirty_chunk_indices(&self) -> Vec<usize> {
        self.dirty_chunks.lock().clone()
    }

    /// Clear the dirty chunk list after GPU has processed updates.
    ///
    /// This should be called after the viewport has re-rendered all dirty chunks.
    pub fn clear_dirty_chunks(&self) {
        self.dirty_chunks.lock().clear();
    }

    /// Enable shared buffer for zero-copy IDE interface (Task D2).
    ///
    /// This creates a SharedVoxelBuffer that tracks dirty pages for
    /// incremental viewport updates. The IDE can query dirty pages to
    /// know which regions need to be re-rendered.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(1000, 1000, 10, test_voxel_size());
    /// grid.enable_shared_buffer();
    /// // Now IDE can query dirty pages for incremental updates
    /// ```
    pub fn enable_shared_buffer(&mut self) {
        if self.shared_buffer.is_none() {
            self.shared_buffer = Some(Arc::new(SharedVoxelBuffer::new(self.size, self.max_chunks)));
        }
    }

    /// Get the shared buffer (if enabled).
    ///
    /// Returns None if shared buffer has not been enabled via enable_shared_buffer().
    pub fn shared_buffer(&self) -> Option<Arc<SharedVoxelBuffer>> {
        self.shared_buffer.as_ref().map(Arc::clone)
    }

    /// Get dirty pages from shared buffer for incremental IDE updates.
    ///
    /// Returns empty vec if shared buffer is not enabled.
    ///
    /// The IDE calls this to determine which memory pages have changed
    /// and need to be re-rendered in the viewport.
    ///
    /// # Performance
    /// O(pages) where pages is typically < 1000 for most designs.
    /// Much faster than scanning all chunks.
    pub fn get_dirty_pages(&self) -> Vec<usize> {
        self.shared_buffer
            .as_ref()
            .map(|buf| buf.get_dirty_pages())
            .unwrap_or_default()
    }

    /// Clear dirty pages after IDE has processed them.
    ///
    /// The IDE should call this after it has re-rendered all dirty regions.
    pub fn clear_dirty_pages(&self) {
        if let Some(buf) = &self.shared_buffer {
            buf.clear_dirty_pages();
        }
    }

    /// Get the number of dirty pages.
    ///
    /// Useful for IDE to decide whether to do incremental or full refresh.
    pub fn count_dirty_pages(&self) -> usize {
        self.shared_buffer
            .as_ref()
            .map(|buf| buf.count_dirty_pages())
            .unwrap_or(0)
    }
}
