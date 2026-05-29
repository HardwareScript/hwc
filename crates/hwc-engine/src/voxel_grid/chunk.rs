//! Voxel chunk implementation - 4x4x4 bitmasked chunks

use crate::netlist::NetHandle;

/// Material ID type (u8 for compact storage).
pub type MaterialId = u8;

/// Net ID type (u32 for up to 4 billion nets).
/// NOTE: VoxelChunks now store NetHandle instead of NetId directly.
/// This enables O(1) net renaming via the NetLookupTable.
pub type NetId = u32;

/// A 4x4x4 Voxel Chunk (exactly 64 voxels).
///
/// Total size: 8 bytes (collision_mask) + 8 bytes (presence_mask) + 64 bytes (materials) + 256 bytes (handles) + 64 bytes (conductivity) = 400 bytes.
/// Still fits in CPU L1 cache (typical L1 is 32-64KB).
///
/// IMPORTANT: Chunks now store NetHandle instead of NetId.
/// This enables O(1) net renaming via the NetLookupTable.
///
/// TWO-LAYER VOXEL SYSTEM (Sprint 1 - v0.1.6):
/// - Occupancy Layer: collision_mask (something is here)
/// - Conductivity Layer: conductivity array (routing rules)
/// - Router uses both layers to distinguish traversable vs blocking materials
///
/// GPU-NATIVE LAYOUT (Task D1):
/// - 16-byte alignment for GPU buffer compatibility
/// - Z-Order (Morton) layout for better GPU cache coherency
/// - Direct GPU access via get_gpu_buffer_ptr()
#[repr(align(16))]
#[derive(Debug, Clone)]
pub(super) struct VoxelChunk {
    /// Each bit represents one of the 64 voxels. 1 = occupied, 0 = air.
    /// This enables O(1) empty chunk detection: if collision_mask == 0, skip entire chunk.
    pub(super) collision_mask: u64,

    /// Bloom filter for NetHandles present in this chunk.
    /// Each bit represents a hash bucket. If bit N is 1, at least one handle hashes to bucket N.
    /// This enables O(1) "does this chunk contain Handle X?" queries for rip-up detection.
    /// False positives are possible (bit is 1 but handle isn't present), but false negatives are impossible.
    pub(super) presence_mask: u64,

    /// Dense arrays for the 64 voxels. Only accessed if the corresponding bit is 1.
    /// NOTE: We store NetHandle (u32) instead of NetId directly.
    pub(super) materials: [MaterialId; 64],
    pub(super) handles: [u32; 64], // Raw u32 for NetHandle storage

    /// Conductivity classification for each voxel (Sprint 1 - Two-Layer System).
    /// 0 = Conductor, 1 = Semiconductor, 2 = Insulator
    /// This enables router to distinguish between blocking conductors and traversable materials.
    pub(super) conductivity: [u8; 64],
}

impl VoxelChunk {
    pub(super) fn new() -> Self {
        Self {
            collision_mask: 0,
            presence_mask: 0,
            materials: [0; 64],
            handles: [0; 64],      // 0 = NetHandle::none()
            conductivity: [2; 64], // Default to Insulator (2)
        }
    }

    /// Maps 3D local coordinates (0-3) to a 1D index (0-63).
    ///
    /// Layout: x + (y * 4) + (z * 16)
    /// This ensures spatial locality within the chunk.
    #[inline(always)]
    pub(super) fn local_index(local_x: usize, local_y: usize, local_z: usize) -> usize {
        local_x + (local_y * 4) + (local_z * 16)
    }

    /// Convert 3D local coordinates to Morton (Z-Order) index.
    ///
    /// Morton encoding interleaves the bits of x, y, z coordinates to create
    /// a 1D index that preserves spatial locality in 3D space.
    /// This improves GPU cache coherency when accessing nearby voxels.
    ///
    /// For a 4x4x4 chunk, each coordinate is 2 bits (0-3).
    /// Morton code: z1 y1 x1 z0 y0 x0 (6 bits total, 0-63)
    ///
    /// Example: (x=1, y=2, z=3) → binary (01, 10, 11) → Morton 111001 = 57
    #[inline(always)]
    #[allow(dead_code)] // Reserved for future GPU shader implementation
    pub(super) fn morton_encode(local_x: usize, local_y: usize, local_z: usize) -> usize {
        // Interleave bits: z1 y1 x1 z0 y0 x0
        let x = local_x & 0x3; // 2 bits
        let y = local_y & 0x3; // 2 bits
        let z = local_z & 0x3; // 2 bits

        // Spread bits: x = 00 00 00 x1 x0
        let x_spread = (x & 0x1) | ((x & 0x2) << 2);
        let y_spread = (y & 0x1) | ((y & 0x2) << 2);
        let z_spread = (z & 0x1) | ((z & 0x2) << 2);

        // Interleave: z1 y1 x1 z0 y0 x0
        x_spread | (y_spread << 1) | (z_spread << 2)
    }

    /// Decode Morton (Z-Order) index back to 3D local coordinates.
    ///
    /// Inverse of morton_encode().
    /// Returns (local_x, local_y, local_z) in range 0-3.
    #[inline(always)]
    #[allow(dead_code)] // Reserved for future GPU shader implementation
    pub(super) fn morton_decode(morton: usize) -> (usize, usize, usize) {
        // Extract interleaved bits
        let x = (morton & 0x1) | ((morton >> 2) & 0x2);
        let y = ((morton >> 1) & 0x1) | ((morton >> 3) & 0x2);
        let z = ((morton >> 2) & 0x1) | ((morton >> 4) & 0x2);

        (x, y, z)
    }

    /// Hash a NetHandle to a bit position in the presence_mask (0-63).
    ///
    /// Uses a simple but effective hash function that distributes handles across all 64 bits.
    /// This is a Bloom filter with 1 hash function.
    #[inline(always)]
    fn hash_handle(handle: NetHandle) -> u32 {
        // Simple multiplicative hash with good distribution
        // The constant is a large prime that provides good mixing
        let hash = handle.raw().wrapping_mul(2654435761u32);
        // Take the top 6 bits to get a value in range 0-63
        (hash >> 26) & 0x3F
    }

    /// Check if a handle might be present in this chunk (O(1) Bloom filter check).
    ///
    /// Returns true if the handle might be present (or false positive).
    /// Returns false if the handle is definitely not present (no false negatives).
    ///
    /// This is the God-Tier O(1) operation for rip-up detection.
    #[inline(always)]
    pub(super) fn might_contain_handle(&self, handle: NetHandle) -> bool {
        let bit_pos = Self::hash_handle(handle);
        (self.presence_mask & (1u64 << bit_pos)) != 0
    }

    /// Add a handle to the presence mask.
    #[inline(always)]
    pub(super) fn add_handle_to_presence(&mut self, handle: NetHandle) {
        let bit_pos = Self::hash_handle(handle);
        self.presence_mask |= 1u64 << bit_pos;
    }

    /// Recalculate the presence mask from scratch by scanning all occupied voxels.
    ///
    /// This is called when voxels are removed to ensure the presence mask stays accurate.
    /// Cost: O(64) but only called when clearing voxels.
    pub(super) fn recalculate_presence_mask(&mut self) {
        self.presence_mask = 0;

        for i in 0..64 {
            if (self.collision_mask & (1u64 << i)) != 0 {
                let handle = NetHandle::new(self.handles[i]);
                if !handle.is_none() {
                    self.add_handle_to_presence(handle);
                }
            }
        }
    }

    /// Get all unique NetHandles present in this chunk.
    ///
    /// This is used for detailed queries after the Bloom filter indicates presence.
    /// Cost: O(64) but only called when we know handles are present.
    pub(super) fn get_unique_handles(&self) -> Vec<NetHandle> {
        let mut handles = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        for i in 0..64 {
            if (self.collision_mask & (1u64 << i)) != 0 {
                let handle = NetHandle::new(self.handles[i]);
                if !handle.is_none() && seen.insert(handle) {
                    handles.push(handle);
                }
            }
        }

        handles
    }
}
