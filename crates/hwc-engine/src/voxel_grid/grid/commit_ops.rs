//! Commit and rollback operations for double-buffered planes

use super::core::VoxelGrid;
use std::sync::atomic::Ordering;

impl VoxelGrid {
    /// Atomically commit the working plane to the visible plane.
    ///
    /// This is the CRITICAL handshake between router and IDE:
    /// - Router writes to working_plane during routing
    /// - When route is 100% complete and passes local DRC, call this
    /// - Safely copies Arc references so IDE sees the new state
    /// - Zero flickering, no partial wires visible
    ///
    /// PERFORMANCE TARGET: < 1 microsecond for the swap
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(100, 100, 10, test_voxel_size());
    /// // Router writes to working plane...
    /// // grid.set_occupied(...);
    /// // When route is complete:
    /// grid.commit_route();
    /// // Now IDE sees the new route
    /// ```
    pub fn commit_route(&self) {
        // Set committing flag to prevent reads during swap
        self.is_committing.store(true, Ordering::SeqCst);

        // GOD-TIER FIX: Only sync dirty chunks instead of scanning all 18.7M slots!
        // Old way: O(Capacity) - iterate all chunk slots even if empty
        // New way: O(Matter) - only sync chunks that were actually modified
        let dirty_indices = self.dirty_chunks.lock().clone();

        // eprintln!($3"[DEBUG commit_route] Syncing {} dirty chunks",
        //  dirty_indices.len());

        // Safe Arc clone for each DIRTY chunk only
        // This is the "Secret Sauce" - O(dirty_chunks) Arc clones, not O(capacity) scans
        for &i in &dirty_indices {
            if let Some(working_chunk) = self.get_working_chunk(i) {
                // Copy working to visible
                self.set_visible_chunk(i, working_chunk);
            } else {
                // Clear visible if working is empty (remove from HashMap)
                if let Ok(mut guard) = self.visible_plane.write() {
                    guard.remove(&i);
                }
            }
        }

        // Clear committing flag
        self.is_committing.store(false, Ordering::SeqCst);
    }

    /// Check if a commit is currently in progress.
    ///
    /// Readers should spin-wait if this returns true (< 1μs).
    /// This prevents reading during the atomic swap.
    #[inline]
    pub fn is_committing(&self) -> bool {
        self.is_committing.load(Ordering::Acquire)
    }

    /// Discard changes in the working plane and reset to visible state.
    ///
    /// This is used when a route fails validation and needs to be rolled back.
    /// The working plane is reset to match the visible plane.
    pub fn rollback_working_plane(&self) {
        // GOD-TIER FIX: Only rollback dirty chunks instead of scanning all slots
        let dirty_indices = self.dirty_chunks.lock().clone();

        // eprintln!($3"[DEBUG rollback_working_plane] Rolling back {} dirty chunks",
        //  dirty_indices.len());

        for &i in &dirty_indices {
            if let Some(visible_chunk) = self.get_visible_chunk(i) {
                // Clone visible to working
                self.set_working_chunk(i, visible_chunk);
            } else {
                // Clear working if visible is empty (remove from HashMap)
                if let Ok(mut guard) = self.working_plane.write() {
                    guard.remove(&i);
                }
            }
        }
    }
}
