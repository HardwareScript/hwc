use crate::connectivity::BoundingBox;
use rustc_hash::FxHashMap;

/// Spatial grid for fast neighbor lookups during flood-fill.
///
/// Divides the space into a grid and maps each cell to the nodes that overlap it.
/// This enables O(1) neighbor lookups instead of O(N) linear search.
pub struct SpatialGrid {
    grid: FxHashMap<(i64, i64, i64), Vec<usize>>,
    cell_size: i64,
}

impl SpatialGrid {
    /// Build a spatial grid index for fast neighbor lookups.
    ///
    /// Grid cell size is chosen to be ~1mm (1,000,000 nm) for typical PCB scales.
    ///
    /// # Arguments
    /// * `nodes` - List of (node_ref, bbox) tuples to index
    ///
    /// # Returns
    /// Spatial grid mapping cells to node indices
    pub fn build(nodes: &[(super::GeometryNodeRef, BoundingBox)]) -> Self {
        let mut grid: FxHashMap<(i64, i64, i64), Vec<usize>> = FxHashMap::default();
        let cell_size = 1_000_000; // 1mm cells

        for (idx, (_node_ref, bbox)) in nodes.iter().enumerate() {
            // Find all grid cells this bbox overlaps
            let min_cell_x = bbox.min_x / cell_size;
            let max_cell_x = bbox.max_x / cell_size;
            let min_cell_y = bbox.min_y / cell_size;
            let max_cell_y = bbox.max_y / cell_size;
            let min_cell_z = bbox.min_z / cell_size;
            let max_cell_z = bbox.max_z / cell_size;

            // Add this node to all overlapping cells
            for cell_x in min_cell_x..=max_cell_x {
                for cell_y in min_cell_y..=max_cell_y {
                    for cell_z in min_cell_z..=max_cell_z {
                        grid.entry((cell_x, cell_y, cell_z)).or_default().push(idx);
                    }
                }
            }
        }

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Built spatial grid with {} cells",
        // grid.len()
        // );

        Self { grid, cell_size }
    }

    /// Get candidate neighbors for a given bounding box.
    ///
    /// Returns all node indices in the same or adjacent grid cells.
    /// Caller must still check for actual touching and material match.
    ///
    /// # Arguments
    /// * `bbox` - Bounding box to find neighbors for
    ///
    /// # Returns
    /// Set of candidate node indices
    pub fn get_candidates(&self, bbox: &BoundingBox) -> rustc_hash::FxHashSet<usize> {
        use rustc_hash::FxHashSet;

        let mut candidates = FxHashSet::default();

        // Find grid cells this bbox overlaps
        let min_cell_x = bbox.min_x / self.cell_size;
        let max_cell_x = bbox.max_x / self.cell_size;
        let min_cell_y = bbox.min_y / self.cell_size;
        let max_cell_y = bbox.max_y / self.cell_size;
        let min_cell_z = bbox.min_z / self.cell_size;
        let max_cell_z = bbox.max_z / self.cell_size;

        // Check all overlapping and adjacent cells
        for cell_x in (min_cell_x - 1)..=(max_cell_x + 1) {
            for cell_y in (min_cell_y - 1)..=(max_cell_y + 1) {
                for cell_z in (min_cell_z - 1)..=(max_cell_z + 1) {
                    if let Some(cell_nodes) = self.grid.get(&(cell_x, cell_y, cell_z)) {
                        for &neighbor_idx in cell_nodes {
                            candidates.insert(neighbor_idx);
                        }
                    }
                }
            }
        }

        candidates
    }
}
