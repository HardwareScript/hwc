use super::super::chunk::VoxelChunk;
use super::super::grid::VoxelGrid;
use crate::geometry::BoundingBox;
use crate::space::VoxelSize;

impl VoxelGrid {
    /// Check if a bounding box collides with existing geometry.
    ///
    /// SPARSE ARCHITECTURE (v0.1.6 Performance Fix):
    /// 1. Check component metadata first: O(components) - typically 10-10,000 components
    /// 2. Skip chunk iteration if working plane is empty (no traces/pours placed yet)
    /// 3. Check voxel chunks only if needed: O(chunks) - for traces/pours
    ///
    /// This fixes the 270ms-per-component bug where we were iterating through
    /// 500,000 empty chunk coordinates for each 8mm x 4mm component.
    ///
    /// # Arguments
    /// * `bbox` - Bounding box in nanometers
    /// * `voxel_size` - Size of each voxel in nanometers
    ///
    /// # Returns
    /// * `Some((x, y, z))` - Voxel coordinates of first collision
    /// * `None` - No collision detected
    pub fn check_bbox_collision(
        &self,
        bbox: &BoundingBox,
        voxel_size: &VoxelSize,
    ) -> Option<(usize, usize, usize)> {
        // PHASE 1: Check component metadata (O(components) - FAST!)
        // Components are stored as sparse metadata, not voxels
        for component in &self.component_metadata {
            if component.bbox.intersects(bbox) {
                // Collision with another component!
                // Return the center of the colliding component as the collision point
                let center_x = (component.bbox.min.x + component.bbox.max.x) / 2;
                let center_y = (component.bbox.min.y + component.bbox.max.y) / 2;
                let center_z = (component.bbox.min.z + component.bbox.max.z) / 2;
                let (vx, vy, vz) = Self::nm_to_voxel(
                    crate::geometry::Point3D::new(center_x, center_y, center_z),
                    voxel_size,
                );
                return Some((vx, vy, vz));
            }
        }

        // PHASE 2: Check if working plane has any voxels at all
        // If the working plane is empty (no traces/pours), skip chunk iteration entirely
        // This is the KEY optimization: don't iterate 500,000 empty chunk coordinates!
        let has_voxels = if let Ok(guard) = self.working_plane.read() {
            !guard.is_empty()
        } else {
            false
        };

        if !has_voxels {
            // No voxels in working plane - no collision possible
            return None;
        }

        // PHASE 3: Check voxel chunks (O(actual_chunks) - SPARSE!)
        // Note: We don't check substrate layers because components are SUPPOSED to sit on substrate

        // Convert nanometer coordinates to voxel coordinates
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

        // GOD-TIER FIX: Iterate through ACTUAL chunks in HashMap, not coordinate ranges!
        // OLD: for chunk_z in min..max (2 million iterations for 8mm×4mm component)
        // NEW: for (chunk_index, chunk) in working_plane (only actual chunks - typically 0-100)
        //
        // This is the "Matter-Centric" approach: check where things ARE, not where they MIGHT be.
        if let Ok(guard) = self.working_plane.read() {
            for (&chunk_index, chunk_arc) in guard.iter() {
                // Convert chunk index back to coordinates
                let (chunk_x, chunk_y, chunk_z) = self.chunk_index_to_coords(chunk_index);

                // Quick bounds check: is this chunk even in our bounding box range?
                if chunk_x < min_chunk_x
                    || chunk_x > max_chunk_x
                    || chunk_y < min_chunk_y
                    || chunk_y > max_chunk_y
                    || chunk_z < min_chunk_z
                    || chunk_z > max_chunk_z
                {
                    continue; // Chunk is outside our bounding box
                }

                // Get the collision mask
                let mask = chunk_arc.collision_mask;
                if mask == 0 {
                    continue; // Empty chunk (shouldn't happen, but safety check)
                }

                // Check if any voxels in this chunk intersect our bounding box
                // This is still O(64) per chunk, but we only check ACTUAL chunks
                for local_z in 0..4 {
                    for local_y in 0..4 {
                        for local_x in 0..4 {
                            let voxel_x = chunk_x * 4 + local_x;
                            let voxel_y = chunk_y * 4 + local_y;
                            let voxel_z = chunk_z * 4 + local_z;

                            // Check if this voxel is within our bounding box
                            if voxel_x >= min_x
                                && voxel_x <= max_x
                                && voxel_y >= min_y
                                && voxel_y <= max_y
                                && voxel_z >= min_z
                                && voxel_z <= max_z
                            {
                                // Check if this voxel is occupied
                                let bit_index = VoxelChunk::local_index(local_x, local_y, local_z);
                                if (mask & (1u64 << bit_index)) != 0 {
                                    // Collision detected!
                                    return Some((voxel_x, voxel_y, voxel_z));
                                }
                            }
                        }
                    }
                }
            }
        }

        None // No collision
    }
}
