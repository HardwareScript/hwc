use super::super::chunk::VoxelChunk;
use super::super::grid::VoxelGrid;
use crate::geometry::BoundingBox;
use crate::space::VoxelSize;
use std::sync::Arc;

impl VoxelGrid {
    /// Clear a voxel (set to empty).
    ///
    /// Uses safe Arc-based pattern for writes.
    /// WRITES TO WORKING PLANE (private memory for router).
    /// Removes the voxel from the chunk. If the chunk becomes empty, it's deleted to reclaim memory.
    /// Recalculates the presence_mask to maintain accuracy.
    /// Marks the chunk and its neighbors as dirty for incremental DRC.
    #[inline]
    pub fn clear(&self, x: usize, y: usize, z: usize) {
        if !self.in_bounds(x, y, z) {
            return;
        }

        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);
        let index = VoxelChunk::local_index(lx, ly, lz);

        // Safe write pattern to WORKING PLANE
        if let Some(chunk_arc) = self.get_working_chunk(chunk_index) {
            // Clone and modify
            let mut new_chunk = (*chunk_arc).clone();

            // Set the bit to 0
            new_chunk.collision_mask &= !(1u64 << index);

            // If the whole chunk is now empty, delete it to reclaim memory
            if new_chunk.collision_mask == 0 {
                // Clear the chunk slot (remove from HashMap)
                if let Ok(mut guard) = self.working_plane.write() {
                    guard.remove(&chunk_index);
                }
            } else {
                // Recalculate presence mask since we removed a voxel
                new_chunk.recalculate_presence_mask();

                // Store the modified chunk
                self.set_working_chunk(chunk_index, Arc::new(new_chunk));
            }
        }

        // Mark this chunk and its neighbors as dirty for incremental DRC
        self.mark_chunk_and_neighbors_dirty(x, y, z);
    }

    /// Clear a bounding box of voxels (Limitation 7).
    ///
    /// This is the inverse of fill_box, used for drilling holes.
    /// It clears the collision_mask bits for all chunks intersecting the bbox.
    pub fn clear_voxels_in_bbox(&mut self, bbox: &BoundingBox) {
        // 1. Convert Physical Nanometers to Integer Voxel Indices
        let x_min = (bbox.min.x / self.voxel_size.x_nm).max(0) as usize;
        let x_max = ((bbox.max.x / self.voxel_size.x_nm).saturating_sub(1)).max(0) as usize;
        let y_min = (bbox.min.y / self.voxel_size.y_nm).max(0) as usize;
        let y_max = ((bbox.max.y / self.voxel_size.y_nm).saturating_sub(1)).max(0) as usize;
        let z_min = (bbox.min.z / self.voxel_size.z_nm).max(0) as usize;
        let z_max = ((bbox.max.z / self.voxel_size.z_nm).saturating_sub(1)).max(0) as usize;

        // Clamp to grid bounds
        let x_min = x_min.min(self.size.0.saturating_sub(1));
        let x_max = x_max.min(self.size.0.saturating_sub(1));
        let y_min = y_min.min(self.size.1.saturating_sub(1));
        let y_max = y_max.min(self.size.1.saturating_sub(1));
        let z_min = z_min.min(self.size.2.saturating_sub(1));
        let z_max = z_max.min(self.size.2.saturating_sub(1));

        // 2. NATIVE LOCK PATTERN: Acquire working plane lock ONCE for entire operation
        if let Ok(mut working_guard) = self.working_plane.write() {
            // 3. Iterate by CHUNKS (4×4×4 blocks), not voxels
            for g_z in z_min..=z_max {
                let chunk_z = g_z / 4;
                let local_z = g_z % 4;

                for chunk_y_idx in (y_min / 4)..=((y_max) / 4) {
                    for chunk_x_idx in (x_min / 4)..=((x_max) / 4) {
                        let chunk_index =
                            self.chunk_coords_to_index(chunk_x_idx, chunk_y_idx, chunk_z);

                        // 4. Calculate the Bitmask for this specific slice of the box
                        let c_x_start = (chunk_x_idx * 4).max(x_min);
                        let c_x_end = ((chunk_x_idx * 4) + 3).min(x_max);
                        let c_y_start = (chunk_y_idx * 4).max(y_min);
                        let c_y_end = ((chunk_y_idx * 4) + 3).min(y_max);

                        // Compute bitmask: row mask shifted by Y and Z offsets
                        let mut chunk_mask: u64 = 0;
                        for gy in c_y_start..=c_y_end {
                            let ly = gy % 4;
                            let lx_start = c_x_start % 4;
                            let lx_end = c_x_end % 4;

                            let row_bits = ((1 << (lx_end - lx_start + 1)) - 1) << lx_start;
                            chunk_mask |= (row_bits as u64) << (local_z * 16 + ly * 4);
                        }

                        if chunk_mask == 0 {
                            continue;
                        }

                        // 5. NATIVE IN-PLACE MUTATION
                        if let Some(chunk_arc) = working_guard.get_mut(&chunk_index) {
                            let chunk = Arc::make_mut(chunk_arc);
                            // Bitwise AND NOT to clear the bits
                            chunk.collision_mask &= !chunk_mask;

                            // If chunk is now empty, we could remove it, but let's keep it simple for now
                            // and just clear bits. router/is_empty handles mask=0 correctly.
                        }
                    }
                }
            }
        }

        // Mark the entire region as dirty
        self.mark_region_dirty(x_min, y_min, z_min, x_max, y_max, z_max);
    }

    /// Clear a bounding box (set all voxels to empty) using GOD-TIER chunk-level operations.
    ///
    /// # Arguments
    /// * `bbox` - Bounding box in nanometers
    /// * `voxel_size` - Size of each voxel in nanometers
    pub fn clear_box(&self, bbox: &BoundingBox, voxel_size: &VoxelSize) {
        let (min_x, min_y, min_z) = Self::nm_to_voxel(bbox.min, voxel_size);
        let (max_x, max_y, max_z) = Self::nm_to_voxel(bbox.max, voxel_size);

        // Clamp to grid bounds
        let min_x = min_x.min(self.size.0.saturating_sub(1));
        let min_y = min_y.min(self.size.1.saturating_sub(1));
        let min_z = min_z.min(self.size.2.saturating_sub(1));

        let max_x = max_x.min(self.size.0.saturating_sub(1));
        let max_y = max_y.min(self.size.1.saturating_sub(1));
        let max_z = max_z.min(self.size.2.saturating_sub(1));

        // Convert to chunk coordinates (chunks are 4×4×4)
        let min_chunk_x = min_x / 4;
        let min_chunk_y = min_y / 4;
        let min_chunk_z = min_z / 4;

        let max_chunk_x = max_x / 4;
        let max_chunk_y = max_y / 4;
        let max_chunk_z = max_z / 4;

        // GOD-TIER: Iterate over chunks, not voxels
        for chunk_z in min_chunk_z..=max_chunk_z {
            for chunk_y in min_chunk_y..=max_chunk_y {
                for chunk_x in min_chunk_x..=max_chunk_x {
                    // Calculate voxel range within this chunk
                    let chunk_min_x = chunk_x * 4;
                    let chunk_min_y = chunk_y * 4;
                    let chunk_min_z = chunk_z * 4;

                    let chunk_max_x = (chunk_x + 1) * 4 - 1;
                    let chunk_max_y = (chunk_y + 1) * 4 - 1;
                    let chunk_max_z = (chunk_z + 1) * 4 - 1;

                    // Intersect with clear region
                    let clear_min_x = min_x.max(chunk_min_x);
                    let clear_min_y = min_y.max(chunk_min_y);
                    let clear_min_z = min_z.max(chunk_min_z);

                    let clear_max_x = max_x.min(chunk_max_x);
                    let clear_max_y = max_y.min(chunk_max_y);
                    let clear_max_z = max_z.min(chunk_max_z);

                    // Check if this chunk is fully contained
                    let fully_contained = clear_min_x == chunk_min_x
                        && clear_min_y == chunk_min_y
                        && clear_min_z == chunk_min_z
                        && clear_max_x == chunk_max_x
                        && clear_max_y == chunk_max_y
                        && clear_max_z == chunk_max_z;

                    if fully_contained {
                        // GOD-TIER FAST PATH: Delete entire chunk (remove from HashMap)
                        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
                        if let Ok(mut guard) = self.working_plane.write() {
                            guard.remove(&chunk_index);
                        }
                    } else {
                        // Edge chunk: Clear voxels individually
                        for z in clear_min_z..=clear_max_z {
                            for y in clear_min_y..=clear_max_y {
                                for x in clear_min_x..=clear_max_x {
                                    self.clear(x, y, z);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
