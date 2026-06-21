use crate::connectivity::BoundingBox;
use rustc_hash::FxHashMap;

pub struct SpatialGrid {
    grid: FxHashMap<(i64, i64, i64), Vec<usize>>,
    cell_size: i64,
}

impl SpatialGrid {
    pub fn build(nodes: &[(super::GeometryNodeRef, BoundingBox)]) -> Self {
        let mut grid: FxHashMap<(i64, i64, i64), Vec<usize>> = FxHashMap::default();
        let cell_size = 1_000_000;

        for (idx, (_node_ref, bbox)) in nodes.iter().enumerate() {
            let min_cell_x = bbox.min_x / cell_size;
            let max_cell_x = bbox.max_x / cell_size;
            let min_cell_y = bbox.min_y / cell_size;
            let max_cell_y = bbox.max_y / cell_size;
            let min_cell_z = bbox.min_z / cell_size;
            let max_cell_z = bbox.max_z / cell_size;

            for cell_x in min_cell_x..=max_cell_x {
                for cell_y in min_cell_y..=max_cell_y {
                    for cell_z in min_cell_z..=max_cell_z {
                        grid.entry((cell_x, cell_y, cell_z)).or_default().push(idx);
                    }
                }
            }
        }

        Self { grid, cell_size }
    }

    pub fn get_candidates(&self, bbox: &BoundingBox) -> rustc_hash::FxHashSet<usize> {
        let mut candidates = rustc_hash::FxHashSet::default();

        let min_cell_x = bbox.min_x / self.cell_size;
        let max_cell_x = bbox.max_x / self.cell_size;
        let min_cell_y = bbox.min_y / self.cell_size;
        let max_cell_y = bbox.max_y / self.cell_size;
        let min_cell_z = bbox.min_z / self.cell_size;
        let max_cell_z = bbox.max_z / self.cell_size;

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
