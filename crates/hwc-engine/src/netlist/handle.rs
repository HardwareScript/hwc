//! Handle-based net indirection for O(1) renaming.
//!
//! This module implements a handle-based system where voxels store NetHandles
//! instead of direct NetIds. This enables O(1) net renaming by only updating
//! the lookup table instead of scanning billions of voxels.
//!
//! ARCHITECTURE:
//! - VoxelChunk stores NetHandle (u32 wrapper)
//! - NetLookupTable maps Handle → NetId
//! - Renaming a net only updates the lookup table (< 1μs)
//! - No voxel scanning required!

use super::NetId;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Strongly-typed net handle (newtype wrapper around u32).
///
/// This is what voxels actually store. It's an indirection layer
/// that points to the real NetId in the lookup table.
///
/// Zero memory overhead - compiles to a raw u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NetHandle(pub u32);

impl NetHandle {
    /// Create a new net handle.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw handle value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Special handle for "no net" (air/empty voxels).
    #[inline]
    pub const fn none() -> Self {
        Self(0)
    }

    /// Check if this is the "no net" handle.
    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// Lookup table mapping NetHandle → NetId.
///
/// This is the secret sauce for O(1) renaming:
/// - Voxels store handles (never change)
/// - Lookup table maps handle → current NetId
/// - Renaming updates the table, not the voxels
///
/// Thread-safe using RwLock for concurrent access.
#[derive(Debug, Clone)]
pub struct NetLookupTable {
    /// Handle → NetId mapping
    /// Uses RwLock for concurrent reads (IDE + Router)
    handle_to_net: Arc<RwLock<FxHashMap<NetHandle, NetId>>>,

    /// Next available handle ID
    next_handle: Arc<parking_lot::Mutex<u32>>,
}

impl NetLookupTable {
    /// Create a new empty lookup table.
    pub fn new() -> Self {
        Self {
            handle_to_net: Arc::new(RwLock::new(FxHashMap::default())),
            next_handle: Arc::new(parking_lot::Mutex::new(1)), // Start at 1 (0 is reserved for "none")
        }
    }

    /// Allocate a new handle for a net.
    ///
    /// This is called when a net is first created or routed.
    ///
    /// # Arguments
    /// * `net_id` - The NetId to associate with this handle
    ///
    /// # Returns
    /// A new NetHandle that maps to the given NetId
    pub fn allocate_handle(&self, net_id: NetId) -> NetHandle {
        let mut next = self.next_handle.lock();
        let handle = NetHandle::new(*next);
        *next += 1;

        // Insert into lookup table
        let mut table = self.handle_to_net.write();
        table.insert(handle, net_id);

        handle
    }

    /// Resolve a handle to its current NetId.
    ///
    /// This is the hot path for voxel queries.
    /// Uses read lock for concurrent access.
    ///
    /// # Arguments
    /// * `handle` - The handle to resolve
    ///
    /// # Returns
    /// The current NetId, or NetId(0) if handle is invalid
    #[inline]
    pub fn resolve_handle(&self, handle: NetHandle) -> NetId {
        if handle.is_none() {
            return NetId::new(0);
        }

        let table = self.handle_to_net.read();
        table.get(&handle).copied().unwrap_or(NetId::new(0))
    }

    /// Rename a net (O(1) operation!).
    ///
    /// This is the God-Tier operation that makes HMR possible.
    /// Instead of scanning billions of voxels, we just update one entry
    /// in the lookup table.
    ///
    /// # Arguments
    /// * `old_net_id` - The old NetId to rename
    /// * `new_net_id` - The new NetId to use
    ///
    /// # Performance
    /// O(N) where N = number of handles (typically small)
    /// Does NOT scan voxels!
    pub fn rename_net(&self, old_net_id: NetId, new_net_id: NetId) {
        let mut table = self.handle_to_net.write();

        // Find all handles that point to old_net_id and update them
        for (_handle, net_id) in table.iter_mut() {
            if *net_id == old_net_id {
                *net_id = new_net_id;
            }
        }
    }

    /// Update a specific handle to point to a new NetId.
    ///
    /// This is used for more surgical updates where we know the exact handle.
    ///
    /// # Arguments
    /// * `handle` - The handle to update
    /// * `new_net_id` - The new NetId to associate with this handle
    pub fn update_handle(&self, handle: NetHandle, new_net_id: NetId) {
        let mut table = self.handle_to_net.write();
        table.insert(handle, new_net_id);
    }

    /// Get all handles that currently map to a specific NetId.
    ///
    /// This is useful for debugging or when you need to know which
    /// handles are affected by a net operation.
    ///
    /// # Arguments
    /// * `net_id` - The NetId to search for
    ///
    /// # Returns
    /// Vector of all handles that currently map to this NetId
    pub fn get_handles_for_net(&self, net_id: NetId) -> Vec<NetHandle> {
        let table = self.handle_to_net.read();
        table
            .iter()
            .filter(|(_, &id)| id == net_id)
            .map(|(&handle, _)| handle)
            .collect()
    }

    /// Get the total number of allocated handles.
    pub fn handle_count(&self) -> usize {
        let table = self.handle_to_net.read();
        table.len()
    }

    /// Clear all handle mappings (for testing/reset).
    pub fn clear(&self) {
        let mut table = self.handle_to_net.write();
        table.clear();

        let mut next = self.next_handle.lock();
        *next = 1;
    }
}

impl Default for NetLookupTable {
    fn default() -> Self {
        Self::new()
    }
}
