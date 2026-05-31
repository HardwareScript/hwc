//! Voxel-level operations (is_empty, get_material, set_occupied, etc.)

use super::core::VoxelGrid;
use crate::netlist::NetHandle;
use crate::voxel_grid::chunk::{MaterialId, NetId, VoxelChunk};
use std::sync::Arc;

impl VoxelGrid {
    /// Check if a voxel is empty (not occupied).
    ///
    /// Ultra-fast bitwise check with flat array lookup (no hashing).
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    ///
    /// SUBSTRATE SPARSE ARCHITECTURE (v0.1.6):
    /// Checks substrate layers FIRST. If substrate exists, voxel is NOT empty.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, VoxelSize, Dimensions, GridCells};
    /// let dims = Dimensions::from_mm(10.0, 10.0, 1.0);
    /// let grid_cells = GridCells::new(100, 100, 10);
    /// let voxel_size = VoxelSize::from_dimensions(dims, grid_cells);
    /// let grid = VoxelGrid::new(100, 100, 10, voxel_size);
    /// assert!(grid.is_empty(5, 5, 5));
    /// ```
    #[inline]
    pub fn is_empty(&self, x: usize, y: usize, z: usize) -> bool {
        if !self.in_bounds(x, y, z) {
            return false;
        }

        // STEP 1: Check substrate layers first (REVERSE order for priority)
        let point = Self::voxel_to_nm(x, y, z, &self.voxel_size);

        for layer in self.substrate_layers.iter().rev() {
            if layer.contains_nm(point.x, point.y, point.z) {
                return false; // Substrate exists, not empty
            }
        }

        // STEP 2: Check sparse chunks
        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);

        // Safe read from VISIBLE plane (stable state)
        if let Some(chunk) = self.get_visible_chunk(chunk_index) {
            let index = VoxelChunk::local_index(lx, ly, lz);
            // Ultra-fast bitwise check. No array lookups needed to verify air!
            return (chunk.collision_mask & (1u64 << index)) == 0;
        }

        true // If no chunk and no substrate, it's pure air
    }

    /// Set a voxel as occupied with material and net handle.
    ///
    /// Uses safe Arc-based pattern for concurrent writes:
    /// 1. Load current chunk
    /// 2. Clone chunk (or create new)
    /// 3. Modify the clone
    /// 4. Store the new Arc
    ///
    /// WRITES TO WORKING PLANE (private memory for router).
    /// Changes are NOT visible to IDE until commit_route() is called.
    ///
    /// Only allocates memory for the chunk if it doesn't exist.
    /// Uses flat array indexing - no HashMap overhead!
    /// Updates the presence_mask for O(1) handle queries.
    /// Marks the chunk and its 26 neighbors as dirty for incremental DRC.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Voxel coordinates
    /// * `material` - Material ID (e.g., 2 for Copper)
    /// * `handle` - NetHandle (indirection to NetId via lookup table)
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, netlist::NetHandle, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(10, 10, 10, test_voxel_size());
    /// let handle = NetHandle::new(1);
    /// grid.set_occupied(5, 5, 5, 2, handle);  // Copper, handle 1
    /// // Note: Not visible until commit_route() is called
    /// ```
    #[inline]
    pub fn set_occupied(
        &self,
        x: usize,
        y: usize,
        z: usize,
        material: MaterialId,
        handle: NetHandle,
    ) {
        if !self.in_bounds(x, y, z) {
            return;
        }

        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);
        let index = VoxelChunk::local_index(lx, ly, lz);

        // Safe write pattern to WORKING PLANE
        // 1. Load current chunk (or create new)
        let mut new_chunk = self
            .get_working_chunk(chunk_index)
            .map(|arc| (*arc).clone())
            .unwrap_or_else(VoxelChunk::new);

        // 2. Modify the chunk
        new_chunk.collision_mask |= 1u64 << index;
        new_chunk.materials[index] = material;
        new_chunk.handles[index] = handle.raw();
        // Note: conductivity is set separately via set_conductivity or defaults to Insulator

        // Update presence mask for O(1) handle queries
        if !handle.is_none() {
            new_chunk.add_handle_to_presence(handle);
        }

        // 3. Store the new chunk
        self.set_working_chunk(chunk_index, Arc::new(new_chunk));

        // Mark this chunk and its neighbors as dirty for incremental DRC
        self.mark_chunk_and_neighbors_dirty(x, y, z);
    }

    /// Set a voxel as occupied with material, net handle, AND conductivity.
    ///
    /// This is the Sprint 1 enhanced version that sets both occupancy and conductivity layers.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Voxel coordinates
    /// * `material` - Material ID
    /// * `handle` - NetHandle
    /// * `conductivity` - MaterialConductivity classification
    #[inline]
    pub fn set_occupied_with_conductivity(
        &self,
        x: usize,
        y: usize,
        z: usize,
        material: MaterialId,
        handle: NetHandle,
        conductivity: crate::voxel::MaterialConductivity,
    ) {
        if !self.in_bounds(x, y, z) {
            return;
        }

        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);
        let index = VoxelChunk::local_index(lx, ly, lz);

        // Safe write pattern to WORKING PLANE
        let mut new_chunk = self
            .get_working_chunk(chunk_index)
            .map(|arc| (*arc).clone())
            .unwrap_or_else(VoxelChunk::new);

        // Set occupancy layer
        new_chunk.collision_mask |= 1u64 << index;
        new_chunk.materials[index] = material;
        new_chunk.handles[index] = handle.raw();

        // Set conductivity layer (Sprint 1 - Two-Layer System)
        new_chunk.conductivity[index] = conductivity as u8;

        // Update presence mask
        if !handle.is_none() {
            new_chunk.add_handle_to_presence(handle);
        }

        // Store the new chunk
        self.set_working_chunk(chunk_index, Arc::new(new_chunk));

        // Mark dirty for incremental DRC
        self.mark_chunk_and_neighbors_dirty(x, y, z);
    }

    /// Get the conductivity classification at a voxel.
    ///
    /// Uses the Sparse-Voxel Handshake (Sprint 1 - Two-Layer System):
    /// 1. Check voxel chunks (traces/components) - O(1)
    /// 2. Check substrate layers (wafers/pours) - O(layers), typically 1-4
    /// 3. Default to Insulator for empty space
    ///
    /// This is the critical method that enables the router to distinguish between:
    /// - Conductors (must avoid if different net)
    /// - Semiconductors (can traverse - substrate material)
    /// - Insulators (can traverse freely)
    ///
    /// # Performance
    /// O(1) for voxel chunks + O(layers) for substrate = O(1) in practice
    /// Much faster than filling 100,000+ voxels!
    #[inline]
    pub fn get_conductivity(
        &self,
        x: usize,
        y: usize,
        z: usize,
        material_registry: &crate::voxel::MaterialRegistry,
    ) -> crate::voxel::MaterialConductivity {
        use crate::voxel::MaterialConductivity;

        if !self.in_bounds(x, y, z) {
            return MaterialConductivity::Insulator;
        }

        // STEP 1: Check voxel chunks (traces/components) - O(1)
        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);
        if let Some(chunk) = self.get_visible_chunk(chunk_index) {
            let index = VoxelChunk::local_index(lx, ly, lz);
            if (chunk.collision_mask & (1u64 << index)) != 0 {
                // Voxel is occupied, get conductivity from material registry
                let material_id = chunk.materials[index];
                return material_registry
                    .get_conductivity(material_id)
                    .unwrap_or(MaterialConductivity::Insulator);
            }
        }

        // STEP 2: Check substrate layers (wafers/pours) - O(layers) - REVERSE for priority
        let point = Self::voxel_to_nm(x, y, z, &self.voxel_size);
        for layer in self.substrate_layers.iter().rev() {
            if layer.contains_nm(point.x, point.y, point.z) {
                // Found in substrate, get conductivity from material registry
                return material_registry
                    .get_conductivity(layer.material)
                    .unwrap_or(MaterialConductivity::Insulator);
            }
        }

        // STEP 3: Default to Insulator for empty space
        MaterialConductivity::Insulator
    }

    /// Get the material at a voxel.
    ///
    /// SPARSE-VOXEL HANDSHAKE (Gap 1.1 Core Principle Fix):
    /// Implements the three-step lookup for "Rust for Atoms" philosophy:
    /// 1. Check high-speed voxel grid (for small routes/gates)
    /// 2. If empty, check substrate_layers (for large wafers/pours)
    /// 3. If both empty, return default_insulator (SiO2/Air)
    ///
    /// This enables:
    /// - Router to "see" obstacles (fixes "0 occupied voxels" problem)
    /// - DRC to detect disconnected nets (Physics Error P41)
    /// - Dielectric auto-fill (Gap 6)
    ///
    /// Returns 0 (Air) if out of bounds.
    /// Uses flat array lookup - no hashing!
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    #[inline]
    pub fn get_material(&self, x: usize, y: usize, z: usize) -> MaterialId {
        if !self.in_bounds(x, y, z) {
            return 0;
        }

        // STEP 1: Check sparse chunks for components/traces FIRST (O(1))
        // Traces and components have HIGHER priority than substrate
        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);

        // Safe read from VISIBLE plane (stable state)
        if let Some(chunk) = self.get_visible_chunk(chunk_index) {
            let index = VoxelChunk::local_index(lx, ly, lz);
            // Only read the array if the bit mask says it's occupied
            if (chunk.collision_mask & (1u64 << index)) != 0 {
                return chunk.materials[index];
            }
        }

        // STEP 2: Check substrate layers as FALLBACK (O(layers) where layers is typically 1-4)
        // PRIORITY FIX (Sprint 1.6): Check in REVERSE order so pours override substrate
        // Pours are added AFTER substrate, so they appear later in the vector
        // Convert voxel coordinates to nanometers (center of voxel)
        let point = Self::voxel_to_nm(x, y, z, &self.voxel_size);

        for layer in self.substrate_layers.iter().rev() {
            if layer.contains_nm(point.x, point.y, point.z) {
                return layer.material;
            }
        }

        // STEP 2.5: Check component metadata (GOD-TIER SPARSE ARCHITECTURE)
        // O(components) lookup - typically 10-10,000 components
        // Components have LOWER priority than pours but HIGHER priority than substrate
        for component in self.component_metadata.iter().rev() {
            if component.contains_nm(point.x, point.y, point.z) {
                return component.material;
            }
        }

        // STEP 3: Return default insulator (SPARSE-VOXEL HANDSHAKE)
        // This is the "Rust for Atoms" principle: empty space is filled with dielectric
        // Router and DRC now "feel" the presence of floating bars and can report disconnection
        self.default_insulator
    }

    /// Get the net handle at a voxel.
    ///
    /// Returns NetHandle::none() if out of bounds or empty.
    /// Uses flat array lookup - no hashing!
    /// Safe read using helper method.
    /// ALWAYS reads from visible_plane (stable state for IDE).
    #[inline]
    pub fn get_net_handle(&self, x: usize, y: usize, z: usize) -> NetHandle {
        if !self.in_bounds(x, y, z) {
            return NetHandle::none();
        }

        // STEP 1: Check sparse chunks for components/traces FIRST (O(1))
        let (chunk_index, lx, ly, lz) = self.get_chunk_and_local_coords(x, y, z);

        // Safe read from VISIBLE plane (stable state)
        if let Some(chunk) = self.get_visible_chunk(chunk_index) {
            let index = VoxelChunk::local_index(lx, ly, lz);
            if (chunk.collision_mask & (1u64 << index)) != 0 {
                return NetHandle::new(chunk.handles[index]);
            }
        }

        // STEP 2: Check substrate layers as FALLBACK (O(layers))
        let point = Self::voxel_to_nm(x, y, z, &self.voxel_size);

        for layer in self.substrate_layers.iter().rev() {
            if layer.contains_nm(point.x, point.y, point.z) {
                return NetHandle::new(layer.net);
            }
        }

        NetHandle::none()
    }

    /// Get the raw handle value at a voxel.
    ///
    /// Returns 0 if out of bounds or empty.
    /// This returns the raw u32 handle value, which can be treated as a NetId
    /// in contexts where NetLookupTable is not available.
    #[inline]
    pub fn get_net(&self, x: usize, y: usize, z: usize) -> NetId {
        self.get_net_handle(x, y, z).raw()
    }

    /// Get the materials of the 6 neighbors (North, South, East, West, Up, Down).
    ///
    /// Returns 0 (Air) for out-of-bounds neighbors.
    /// Due to chunking, neighbors are likely in the same chunk/cache line.
    #[inline]
    pub fn get_neighbors(&self, x: usize, y: usize, z: usize) -> [MaterialId; 6] {
        [
            self.get_material(x, y.wrapping_add(1), z), // North
            self.get_material(x, y.wrapping_sub(1), z), // South
            self.get_material(x.wrapping_add(1), y, z), // East
            self.get_material(x.wrapping_sub(1), y, z), // West
            self.get_material(x, y, z.wrapping_add(1)), // Up
            self.get_material(x, y, z.wrapping_sub(1)), // Down
        ]
    }

    /// Get neighbor occupancy info using batch coordinate calculation.
    ///
    /// This function calculates all 6 neighbor coordinates at once,
    /// which enables better cache locality when checking multiple neighbors.
    /// Safe reads using helper methods.
    ///
    /// Returns: [bool; 6] where true = empty, false = occupied
    /// Order: [North, South, East, West, Up, Down]
    ///
    /// # Performance
    /// This may be faster than 6 individual is_empty() calls due to:
    /// - Batch coordinate calculation
    /// - Better cache locality when checking multiple neighbors
    #[inline]
    pub fn get_neighbors_info(&self, x: usize, y: usize, z: usize) -> [bool; 6] {
        // Calculate neighbor coordinates
        let neighbors = [
            (x, y.wrapping_add(1), z), // North
            (x, y.wrapping_sub(1), z), // South
            (x.wrapping_add(1), y, z), // East
            (x.wrapping_sub(1), y, z), // West
            (x, y, z.wrapping_add(1)), // Up
            (x, y, z.wrapping_sub(1)), // Down
        ];

        // Bounds check
        let in_bounds: [bool; 6] = [
            self.in_bounds(neighbors[0].0, neighbors[0].1, neighbors[0].2),
            self.in_bounds(neighbors[1].0, neighbors[1].1, neighbors[1].2),
            self.in_bounds(neighbors[2].0, neighbors[2].1, neighbors[2].2),
            self.in_bounds(neighbors[3].0, neighbors[3].1, neighbors[3].2),
            self.in_bounds(neighbors[4].0, neighbors[4].1, neighbors[4].2),
            self.in_bounds(neighbors[5].0, neighbors[5].1, neighbors[5].2),
        ];

        // Get chunk indices and local coordinates for all 6 neighbors
        let mut chunk_indices = [0usize; 6];
        let mut local_coords = [(0usize, 0usize, 0usize); 6];

        for i in 0..6 {
            if in_bounds[i] {
                let (chunk_idx, lx, ly, lz) =
                    self.get_chunk_and_local_coords(neighbors[i].0, neighbors[i].1, neighbors[i].2);
                chunk_indices[i] = chunk_idx;
                local_coords[i] = (lx, ly, lz);
            }
        }

        // Check occupancy for all 6 neighbors
        let mut result = [true; 6]; // Default to empty

        for i in 0..6 {
            if !in_bounds[i] {
                result[i] = false; // Out of bounds = not empty
                continue;
            }

            // Safe read from VISIBLE plane (stable state)
            if let Some(chunk) = self.get_visible_chunk(chunk_indices[i]) {
                let (lx, ly, lz) = local_coords[i];
                let index = VoxelChunk::local_index(lx, ly, lz);
                result[i] = (chunk.collision_mask & (1u64 << index)) == 0;
            }
            // else: no chunk, result[i] stays true (empty)
        }

        result
    }

    /// Check if a point (in nanometers) is inside a keepout zone (KOZ).
    ///
    /// Layer-Aware Keepout (v0.1.7):
    /// - Checks both `SubstrateLayer::is_in_koz` (TSV stress KOZ) and
    ///   `ComponentMetadata::is_in_koz` (component body KOZ with blocked_z_ranges)
    /// - Returns `true` if the point is inside ANY keepout zone.
    ///
    /// This is used by `stamp_pour` / trace placement to determine if a voxel
    /// should be blocked. Without this check, pours and traces would collide
    /// with components on ALL Z-layers (the "Blunt Keepout" problem).
    ///
    /// With Layer-Aware KOZ, a pour on M1 can flow under a component on M3
    /// because the component's `blocked_z_ranges` only covers M3's Z-range.
    ///
    /// # Arguments
    /// * `x_nm`, `y_nm`, `z_nm` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if point is inside any keepout zone
    #[inline]
    pub fn is_inside_keepout_zone(&self, x_nm: i64, y_nm: i64, z_nm: i64) -> bool {
        // 1. Check substrate layer KOZ (TSV stress fields, via KOZ radii)
        for layer in self.substrate_layers.iter() {
            if layer.is_in_koz(x_nm, y_nm, z_nm) {
                return true;
            }
        }

        // 2. Check component metadata KOZ (Layer-Aware blocked Z-ranges)
        for component in self.component_metadata.iter() {
            if component.is_in_koz(x_nm, y_nm, z_nm) {
                return true;
            }
        }

        false
    }

    /// Iterate over all occupied voxels efficiently.
    ///
    /// This is MUCH faster than scanning the entire grid, especially for sparse grids.
    /// Only iterates over non-null chunks and occupied voxels within those chunks.
    /// Safe reads using helper methods.
    ///
    /// Returns an iterator of (x, y, z, material, handle) tuples.
    ///
    /// OPTIMIZED: Only iterates over occupied chunks (collision_mask != 0),
    /// skipping millions of empty voxels for massive performance gains.
    pub fn iter_occupied(
        &self,
    ) -> impl Iterator<Item = (usize, usize, usize, MaterialId, NetHandle)> + '_ {
        let mut occupied = Vec::new();

        // CRITICAL OPTIMIZATION: Only iterate over chunks that actually exist in the HashMap!
        // Don't iterate over all possible chunk coordinates - that's O(grid_size^3)
        // Instead, iterate over the sparse HashMap keys - that's O(occupied_chunks)
        let visible = self.visible_plane.read().unwrap();

        for (&chunk_index, chunk) in visible.iter() {
            // Skip chunks with no occupied voxels
            if chunk.collision_mask == 0 {
                continue;
            }

            // Convert chunk index back to chunk coordinates
            let (chunk_x_count, chunk_y_count, _chunk_z_count) = self.chunk_dimensions();
            let chunk_x = chunk_index % chunk_x_count;
            let chunk_y = (chunk_index / chunk_x_count) % chunk_y_count;
            let chunk_z = chunk_index / (chunk_x_count * chunk_y_count);

            // Iterate over voxels in this chunk
            for local_z in 0..4 {
                for local_y in 0..4 {
                    for local_x in 0..4 {
                        // Convert local chunk coords to global coords
                        let x = chunk_x * 4 + local_x;
                        let y = chunk_y * 4 + local_y;
                        let z = chunk_z * 4 + local_z;

                        // Bounds check
                        if x >= self.size.0 || y >= self.size.1 || z >= self.size.2 {
                            continue;
                        }

                        // Check if this voxel is occupied using the collision mask
                        let local_index = VoxelChunk::local_index(local_x, local_y, local_z);
                        if (chunk.collision_mask & (1u64 << local_index)) != 0 {
                            let material = chunk.materials[local_index];
                            let handle = NetHandle(chunk.handles[local_index]);
                            occupied.push((x, y, z, material, handle));
                        }
                    }
                }
            }
        }

        occupied.into_iter()
    }
}
