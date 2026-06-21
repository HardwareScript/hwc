//! Bit-Parallel Physics Buffers - God-Tier collision detection.
//!
//! This module implements bit-plane storage for ultra-fast parallel physics validation.
//! Instead of checking voxels one-by-one, we use bitwise operations to check 64 voxels
//! simultaneously.
//!
//! KEY INNOVATIONS:
//! - 64-voxel parallelism: A single `&` (AND) operation checks 64 voxels at once
//! - Material bit-planes: Instead of array of u8 materials, use separate bitmasks per material
//! - Net bit-planes: Separate bitmasks for each net enable instant short-circuit detection
//! - Collision detection: `(copper_plane & net_a_plane) & (copper_plane & net_b_plane)` finds shorts
//! - Zero-copy queries: All operations work directly on u64 bitmasks (no array iteration)
//!
//! PERFORMANCE:
//! - Traditional: Check 64 voxels = 64 array lookups + 64 comparisons
//! - Bit-plane: Check 64 voxels = 1 bitwise AND operation
//! - Expected speedup: 50-100× for collision detection
//!
//! ARCHITECTURE:
//! Each VoxelChunk (4×4×4 = 64 voxels) stores:
//! - collision_mask: u64 (which voxels are occupied)
//! - material_planes: FxHashMap<MaterialId, u64> (which voxels have each material)
//! - net_planes: FxHashMap<NetId, u64> (which voxels belong to each net)
//!
//! MEMORY SCALING:
//! - Empty chunk: 8 bytes (just collision_mask)
//! - Chunk with 1 material: 8 + 8 = 16 bytes
//! - Chunk with 5 materials, 10 nets: 8 + (5×8) + (10×8) = 128 bytes
//! - Still fits in L1 cache!

use rustc_hash::FxHashMap;

use crate::geometry_router::substrate_types::{MaterialId, NetId};

/// A 4×4×4 Voxel Chunk with bit-plane storage (exactly 64 voxels).
///
/// Uses bit-planes for ultra-fast parallel collision detection.
/// Each material and net has its own u64 bitmask.
///
/// Total size: 8 bytes (collision_mask) + dynamic HashMaps for materials/nets
/// Typical size: 16-128 bytes (fits in L1 cache)
#[derive(Debug, Clone)]
pub struct BitChunk {
    /// Each bit represents one of the 64 voxels. 1 = occupied, 0 = air.
    /// This enables O(1) empty chunk detection: if collision_mask == 0, skip entire chunk.
    pub collision_mask: u64,

    /// Material bit-planes: One u64 bitmask per material type.
    /// Key = MaterialId, Value = u64 bitmask of which voxels have that material.
    ///
    /// Example: If material 2 (Copper) occupies voxels 0, 5, and 10:
    /// material_planes[2] = 0b...0000010000100001
    pub material_planes: FxHashMap<MaterialId, u64>,

    /// Net bit-planes: One u64 bitmask per net.
    /// Key = NetId, Value = u64 bitmask of which voxels belong to that net.
    ///
    /// Example: If net 100 occupies voxels 0, 1, 2:
    /// net_planes[100] = 0b...0000000000000111
    pub net_planes: FxHashMap<NetId, u64>,
}

impl BitChunk {
    /// Create a new empty BitChunk.
    pub fn new() -> Self {
        Self {
            collision_mask: 0,
            material_planes: FxHashMap::default(),
            net_planes: FxHashMap::default(),
        }
    }

    /// Maps 3D local coordinates (0-3) to a 1D index (0-63).
    ///
    /// Layout: x + (y * 4) + (z * 16)
    /// This ensures spatial locality within the chunk.
    #[inline(always)]
    pub fn local_index(local_x: usize, local_y: usize, local_z: usize) -> usize {
        local_x + (local_y * 4) + (local_z * 16)
    }

    /// Set a voxel as occupied with material and net ID.
    ///
    /// Updates all three bit-planes: collision, material, and net.
    ///
    /// # Arguments
    /// * `index` - Local voxel index (0-63)
    /// * `material` - Material ID
    /// * `net` - Net ID
    #[inline]
    pub fn set_occupied(&mut self, index: usize, material: MaterialId, net: NetId) {
        debug_assert!(index < 64, "Voxel index must be 0-63");

        let bit = 1u64 << index;

        // Set collision bit
        self.collision_mask |= bit;

        // Set material bit-plane
        *self.material_planes.entry(material).or_insert(0) |= bit;

        // Set net bit-plane
        *self.net_planes.entry(net).or_insert(0) |= bit;
    }

    /// Clear a voxel (set to empty).
    ///
    /// Removes the voxel from all bit-planes.
    ///
    /// # Arguments
    /// * `index` - Local voxel index (0-63)
    /// * `material` - Material ID to remove
    /// * `net` - Net ID to remove
    #[inline]
    pub fn clear(&mut self, index: usize, material: MaterialId, net: NetId) {
        debug_assert!(index < 64, "Voxel index must be 0-63");

        let bit = !(1u64 << index);

        // Clear collision bit
        self.collision_mask &= bit;

        // Clear material bit-plane
        if let Some(plane) = self.material_planes.get_mut(&material) {
            *plane &= bit;
            if *plane == 0 {
                self.material_planes.remove(&material);
            }
        }

        // Clear net bit-plane
        if let Some(plane) = self.net_planes.get_mut(&net) {
            *plane &= bit;
            if *plane == 0 {
                self.net_planes.remove(&net);
            }
        }
    }

    /// Check if a voxel is occupied.
    ///
    /// Ultra-fast bitwise check.
    #[inline]
    pub fn is_occupied(&self, index: usize) -> bool {
        debug_assert!(index < 64, "Voxel index must be 0-63");
        (self.collision_mask & (1u64 << index)) != 0
    }

    /// Check if the entire chunk is empty.
    ///
    /// O(1) operation - just check if collision_mask is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.collision_mask == 0
    }

    /// Get the material at a voxel index.
    ///
    /// Returns None if the voxel is empty or material not found.
    #[inline]
    pub fn get_material(&self, index: usize) -> Option<MaterialId> {
        debug_assert!(index < 64, "Voxel index must be 0-63");

        if !self.is_occupied(index) {
            return None;
        }

        let bit = 1u64 << index;
        for (&material, &plane) in &self.material_planes {
            if (plane & bit) != 0 {
                return Some(material);
            }
        }

        None
    }

    /// Get the net ID at a voxel index.
    ///
    /// Returns None if the voxel is empty or net not found.
    #[inline]
    pub fn get_net(&self, index: usize) -> Option<NetId> {
        debug_assert!(index < 64, "Voxel index must be 0-63");

        if !self.is_occupied(index) {
            return None;
        }

        let bit = 1u64 << index;
        for (&net, &plane) in &self.net_planes {
            if (plane & bit) != 0 {
                return Some(net);
            }
        }

        None
    }

    /// Get the bitmask for a specific material.
    ///
    /// Returns 0 if the material is not present in this chunk.
    #[inline]
    pub fn get_material_plane(&self, material: MaterialId) -> u64 {
        self.material_planes.get(&material).copied().unwrap_or(0)
    }

    /// Get the bitmask for a specific net.
    ///
    /// Returns 0 if the net is not present in this chunk.
    #[inline]
    pub fn get_net_plane(&self, net: NetId) -> u64 {
        self.net_planes.get(&net).copied().unwrap_or(0)
    }

    /// Detect collision between two nets in this chunk.
    ///
    /// Returns true if both nets occupy the same voxel (short circuit).
    ///
    /// This is the killer feature: Check 64 voxels with a single AND operation!
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::BitChunk;
    /// let mut chunk = BitChunk::new();
    /// chunk.set_occupied(0, 2, 100); // Copper, net 100
    /// chunk.set_occupied(1, 2, 200); // Copper, net 200
    /// chunk.set_occupied(2, 2, 100); // Copper, net 100
    ///
    /// assert!(!chunk.has_net_collision(100, 200)); // Different voxels, no collision
    ///
    /// chunk.set_occupied(0, 2, 200); // Now both nets at voxel 0
    /// assert!(chunk.has_net_collision(100, 200)); // Collision detected!
    /// ```
    #[inline]
    pub fn has_net_collision(&self, net_a: NetId, net_b: NetId) -> bool {
        let plane_a = self.get_net_plane(net_a);
        let plane_b = self.get_net_plane(net_b);

        // If any bit is set in both planes, there's a collision
        (plane_a & plane_b) != 0
    }

    /// Detect short circuits between a conductive net and a substrate material.
    ///
    /// This is specifically for TSV validation: checks if a conductive core
    /// touches a substrate material (like Silicon) without an insulator liner.
    ///
    /// # Arguments
    /// * `net_id` - The net ID to check (conductive net)
    /// * `substrate_material` - The material ID of the substrate (e.g. Silicon)
    ///
    /// # Returns
    /// Bitmask of voxels where the net and substrate overlap (violation)
    #[inline]
    pub fn find_substrate_shorts(&self, net_id: NetId, substrate_material: MaterialId) -> u64 {
        let net_plane = self.get_net_plane(net_id);
        let substrate_plane = self.get_material_plane(substrate_material);

        net_plane & substrate_plane
    }

    /// Detect collision between a material and a net.
    ///
    /// Returns the bitmask of voxels where the material and net overlap.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::BitChunk;
    /// let mut chunk = BitChunk::new();
    /// chunk.set_occupied(0, 2, 100); // Copper, net 100
    /// chunk.set_occupied(1, 3, 100); // Silicon, net 100
    ///
    /// let copper_overlap = chunk.material_net_overlap(2, 100);
    /// assert_eq!(copper_overlap, 0b1); // Only voxel 0 has copper + net 100
    ///
    /// let silicon_overlap = chunk.material_net_overlap(3, 100);
    /// assert_eq!(silicon_overlap, 0b10); // Only voxel 1 has silicon + net 100
    /// ```
    #[inline]
    pub fn material_net_overlap(&self, material: MaterialId, net: NetId) -> u64 {
        let material_plane = self.get_material_plane(material);
        let net_plane = self.get_net_plane(net);

        material_plane & net_plane
    }

    /// Find all short circuits in this chunk.
    ///
    /// Returns a vector of (net_a, net_b, collision_mask) tuples.
    /// The collision_mask shows which voxels have the collision.
    ///
    /// This is ultra-fast: O(N²) where N = number of nets in chunk (typically 1-10).
    pub fn find_all_short_circuits(&self) -> Vec<(NetId, NetId, u64)> {
        let mut collisions = Vec::new();

        let nets: Vec<NetId> = self.net_planes.keys().copied().collect();

        for i in 0..nets.len() {
            for j in (i + 1)..nets.len() {
                let net_a = nets[i];
                let net_b = nets[j];

                let plane_a = self.get_net_plane(net_a);
                let plane_b = self.get_net_plane(net_b);

                let collision_mask = plane_a & plane_b;
                if collision_mask != 0 {
                    collisions.push((net_a, net_b, collision_mask));
                }
            }
        }

        collisions
    }

    /// Count the number of occupied voxels in this chunk.
    ///
    /// Uses the population count (popcount) instruction for maximum speed.
    #[inline]
    pub fn count_occupied(&self) -> u32 {
        self.collision_mask.count_ones()
    }

    /// Get all materials present in this chunk.
    pub fn get_materials(&self) -> Vec<MaterialId> {
        self.material_planes.keys().copied().collect()
    }

    /// Get all nets present in this chunk.
    pub fn get_nets(&self) -> Vec<NetId> {
        self.net_planes.keys().copied().collect()
    }
}

impl Default for BitChunk {
    fn default() -> Self {
        Self::new()
    }
}
