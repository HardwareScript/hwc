use super::super::grid::VoxelGrid;

impl VoxelGrid {
    /// Mark an entire region as dirty (for bulk operations).
    ///
    /// This is more efficient than marking each chunk individually.
    ///
    /// # Arguments
    /// * `min_x`, `min_y`, `min_z` - Minimum voxel coordinates
    /// * `max_x`, `max_y`, `max_z` - Maximum voxel coordinates
    pub(crate) fn mark_region_dirty(
        &self,
        min_x: usize,
        min_y: usize,
        min_z: usize,
        max_x: usize,
        max_y: usize,
        max_z: usize,
    ) {
        // Convert to chunk coordinates
        let min_chunk_x = min_x / 4;
        let min_chunk_y = min_y / 4;
        let min_chunk_z = min_z / 4;

        let max_chunk_x = max_x / 4;
        let max_chunk_y = max_y / 4;
        let max_chunk_z = max_z / 4;

        // Mark all chunks in the region (plus 1-chunk border for neighbors)
        let mut dirty_chunks = self.dirty_chunks.lock();

        for chunk_z in min_chunk_z.saturating_sub(1)..=(max_chunk_z + 1) {
            for chunk_y in min_chunk_y.saturating_sub(1)..=(max_chunk_y + 1) {
                for chunk_x in min_chunk_x.saturating_sub(1)..=(max_chunk_x + 1) {
                    let (chunks_x, chunks_y, chunks_z) = self.chunk_dimensions();
                    if chunk_x < chunks_x && chunk_y < chunks_y && chunk_z < chunks_z {
                        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
                        if !dirty_chunks.contains(&chunk_index) {
                            dirty_chunks.push(chunk_index);
                        }
                    }
                }
            }
        }
    }

    /// Mark a chunk and its 26 neighbors as dirty for incremental DRC.
    ///
    /// This is called automatically by `set_occupied()` and `clear()`.
    /// Physics validation will only check dirty chunks, dramatically reducing validation time.
    /// Thread-safe using mutex.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Voxel coordinates (not chunk coordinates)
    pub fn mark_chunk_and_neighbors_dirty(&self, x: usize, y: usize, z: usize) {
        let (chunk_x, chunk_y, chunk_z) = Self::voxel_to_chunk(x, y, z);

        // Mark the chunk itself
        self.mark_chunk_dirty_by_coords(chunk_x, chunk_y, chunk_z);

        // Mark all 26 neighbors (3×3×3 cube minus center)
        for dz in -1..=1i32 {
            for dy in -1..=1i32 {
                for dx in -1..=1i32 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue; // Skip center (already marked)
                    }

                    let nx = chunk_x as i32 + dx;
                    let ny = chunk_y as i32 + dy;
                    let nz = chunk_z as i32 + dz;

                    // Check bounds
                    if nx >= 0 && ny >= 0 && nz >= 0 {
                        let (chunks_x, chunks_y, chunks_z) = self.chunk_dimensions();
                        if (nx as usize) < chunks_x
                            && (ny as usize) < chunks_y
                            && (nz as usize) < chunks_z
                        {
                            self.mark_chunk_dirty_by_coords(nx as usize, ny as usize, nz as usize);
                        }
                    }
                }
            }
        }
    }

    /// Mark a specific chunk as dirty by chunk coordinates.
    ///
    /// Thread-safe using mutex.
    /// Checks working plane to see if chunk exists.
    /// Also marks the corresponding page dirty in shared buffer (if enabled).
    ///
    /// # Arguments
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates (not voxel coordinates)
    fn mark_chunk_dirty_by_coords(&self, chunk_x: usize, chunk_y: usize, chunk_z: usize) {
        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);

        // Check if chunk exists in working plane
        if self.get_working_chunk(chunk_index).is_none() {
            // Note: We don't mark non-existent chunks as dirty because empty space
            // doesn't need physics validation
            return;
        }

        // Mark the chunk as dirty using thread-safe mutex
        let mut dirty_chunks = self.dirty_chunks.lock();
        if !dirty_chunks.contains(&chunk_index) {
            dirty_chunks.push(chunk_index);
        }

        // Also mark the page dirty in shared buffer (if enabled)
        // This enables incremental IDE viewport updates
        if let Some(shared_buf) = &self.shared_buffer {
            shared_buf.mark_chunk_dirty(chunk_index);
        }
    }

    /// Get the list of dirty chunk indices.
    ///
    /// This is used by PhysicsValidator to perform incremental validation.
    /// Thread-safe using mutex.
    ///
    /// # Returns
    /// Vec of chunk indices that need validation
    pub fn get_dirty_chunks(&self) -> Vec<usize> {
        self.dirty_chunks.lock().clone()
    }

    /// Clear all dirty flags after physics validation completes.
    ///
    /// This should be called by PhysicsValidator after successful validation.
    /// Thread-safe using mutex.
    pub fn clear_dirty_flags(&self) {
        self.dirty_chunks.lock().clear();
    }

    /// Mark all chunks as dirty (for full board validation).
    ///
    /// This is useful when you need to force a complete re-validation,
    /// such as after loading a design or changing global constraints.
    /// Thread-safe using mutex.
    /// Marks chunks from working plane.
    pub fn mark_all_dirty(&mut self) {
        let mut dirty_chunks = self.dirty_chunks.lock();
        dirty_chunks.clear();

        if let Ok(working_guard) = self.working_plane.read() {
            for index in working_guard.keys() {
                dirty_chunks.push(*index);
            }
        }
    }
}
