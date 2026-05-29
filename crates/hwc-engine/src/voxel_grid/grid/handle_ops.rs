//! Handle and net query operations

use super::core::VoxelGrid;
use crate::netlist::NetHandle;
use crate::voxel_grid::chunk::NetId;

impl VoxelGrid {
    /// Check if a chunk might contain a specific handle (O(1) Bloom filter check).
    ///
    /// This is the God-Tier operation for rip-up detection during component dragging.
    /// Returns true if the handle might be present (or false positive).
    /// Returns false if the handle is definitely not present (no false negatives).
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates (not voxel coordinates!)
    /// * `handle` - NetHandle to check for
    ///
    /// # Performance
    /// O(1) - single bitwise AND operation
    #[inline]
    pub fn chunk_might_contain_handle(
        &self,
        chunk_x: usize,
        chunk_y: usize,
        chunk_z: usize,
        handle: NetHandle,
    ) -> bool {
        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
        self.get_visible_chunk(chunk_index)
            .map(|chunk| chunk.might_contain_handle(handle))
            .unwrap_or(false)
    }

    /// Check if a chunk might contain a specific net (by raw ID).
    ///
    /// Convenience method that wraps the net_id in a NetHandle.
    #[inline]
    pub fn chunk_might_contain_net(
        &self,
        chunk_x: usize,
        chunk_y: usize,
        chunk_z: usize,
        net_id: NetId,
    ) -> bool {
        self.chunk_might_contain_handle(chunk_x, chunk_y, chunk_z, NetHandle::new(net_id))
    }

    /// Get all unique handles present in a chunk.
    ///
    /// This is used after the Bloom filter indicates presence to get the exact list.
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// # Arguments
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates (not voxel coordinates!)
    ///
    /// # Performance
    /// O(64) - scans all voxels in the chunk
    pub fn get_handles_in_chunk(
        &self,
        chunk_x: usize,
        chunk_y: usize,
        chunk_z: usize,
    ) -> Vec<NetHandle> {
        let chunk_index = self.chunk_coords_to_index(chunk_x, chunk_y, chunk_z);
        self.get_visible_chunk(chunk_index)
            .map(|chunk| chunk.get_unique_handles())
            .unwrap_or_default()
    }

    /// Get all nets in a chunk (returns raw handle values).
    ///
    /// Convenience method that returns raw u32 values instead of NetHandle.
    pub fn get_nets_in_chunk(&self, chunk_x: usize, chunk_y: usize, chunk_z: usize) -> Vec<NetId> {
        self.get_handles_in_chunk(chunk_x, chunk_y, chunk_z)
            .into_iter()
            .map(|h| h.raw())
            .collect()
    }

    /// Get all handles that intersect with a bounding box (for component collision detection).
    ///
    /// This is the high-level API for "What's Under My Feet?" queries during HMR.
    ///
    /// # Arguments
    /// * `min` - Minimum corner (x, y, z) in voxels
    /// * `max` - Maximum corner (x, y, z) in voxels
    ///
    /// # Performance
    /// O(chunks_in_box × 64) worst case, but Bloom filter eliminates most empty chunks
    pub fn get_handles_in_region(
        &self,
        min: (usize, usize, usize),
        max: (usize, usize, usize),
    ) -> Vec<NetHandle> {
        let (min_x, min_y, min_z) = min;
        let (max_x, max_y, max_z) = max;

        // Convert to chunk coordinates
        let (min_chunk_x, min_chunk_y, min_chunk_z) = Self::voxel_to_chunk(min_x, min_y, min_z);
        let (max_chunk_x, max_chunk_y, max_chunk_z) = Self::voxel_to_chunk(max_x, max_y, max_z);

        let mut all_handles = rustc_hash::FxHashSet::default();

        // Scan all chunks in the bounding box
        for chunk_z in min_chunk_z..=max_chunk_z {
            for chunk_y in min_chunk_y..=max_chunk_y {
                for chunk_x in min_chunk_x..=max_chunk_x {
                    let handles = self.get_handles_in_chunk(chunk_x, chunk_y, chunk_z);
                    all_handles.extend(handles);
                }
            }
        }

        all_handles.into_iter().collect()
    }

    /// Get all nets in a region (returns raw handle values).
    ///
    /// Convenience method that returns raw u32 values instead of NetHandle.
    pub fn get_nets_in_region(
        &self,
        min: (usize, usize, usize),
        max: (usize, usize, usize),
    ) -> Vec<NetId> {
        self.get_handles_in_region(min, max)
            .into_iter()
            .map(|h| h.raw())
            .collect()
    }
}
