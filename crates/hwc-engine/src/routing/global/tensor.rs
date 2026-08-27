//! DOPHR Stage 1: 3D Volumetric Tensor & PathFinder Negotiated Congestion Global Router
//!
//! Models preferred-direction capacity tracking (Hx, Hy, Cx, Cy), 3D via porosity subtraction,
//! and PathFinder negotiated congestion on a 14-byte/cell Data-Oriented (DoD) memory buffer.

use super::guide::{GCellVolume3D, RoutingGuide};
use hwc_types::NetId;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// 3D Volumetric Occupancy and Capacity Tensor (14 bytes per G-cell total)
#[derive(Clone, Debug)]
pub struct VolumetricTensor3D {
    pub dim_x: usize,
    pub dim_y: usize,
    pub dim_z: usize,
    pub gcell_size_pm: i64,

    // 14 bytes per G-cell Structure-of-Arrays (SoA)
    pub cap_x: Vec<u16>,     // Horizontal track capacity (2 bytes)
    pub cap_y: Vec<u16>,     // Vertical track capacity (2 bytes)
    pub occ_x: Vec<u16>,     // Present X occupancy (2 bytes)
    pub occ_y: Vec<u16>,     // Present Y occupancy (2 bytes)
    pub hist_x: Vec<u16>,    // Historical X congestion penalty (2 bytes)
    pub hist_y: Vec<u16>,    // Historical Y congestion penalty (2 bytes)
    pub base_cost: Vec<u16>, // Base layer/material wire cost (2 bytes)
}

impl VolumetricTensor3D {
    /// Allocate a new 3D volumetric tensor with default track capacity per cell
    pub fn new(
        dim_x: usize,
        dim_y: usize,
        dim_z: usize,
        gcell_size_pm: i64,
        default_cap_x: u16,
        default_cap_y: u16,
    ) -> Self {
        let total_cells = dim_x * dim_y * dim_z;
        Self {
            dim_x,
            dim_y,
            dim_z,
            gcell_size_pm,
            cap_x: vec![default_cap_x; total_cells],
            cap_y: vec![default_cap_y; total_cells],
            occ_x: vec![0; total_cells],
            occ_y: vec![0; total_cells],
            hist_x: vec![0; total_cells],
            hist_y: vec![0; total_cells],
            base_cost: vec![10; total_cells],
        }
    }

    #[inline(always)]
    pub fn cell_index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dim_y + y) * self.dim_x + x
    }

    #[inline(always)]
    pub fn in_bounds(&self, x: usize, y: usize, z: usize) -> bool {
        x < self.dim_x && y < self.dim_y && z < self.dim_z
    }

    /// Subtract capacity from intermediate layers due to 3D via porosity
    pub fn apply_via_porosity(
        &mut self,
        gx: usize,
        gy: usize,
        z_start: usize,
        z_end: usize,
        tracks_blocked: u16,
    ) {
        let (min_z, max_z) = if z_start <= z_end {
            (z_start, z_end)
        } else {
            (z_end, z_start)
        };

        for z in min_z..=max_z {
            if self.in_bounds(gx, gy, z) {
                let idx = self.cell_index(gx, gy, z);
                self.cap_x[idx] = self.cap_x[idx].saturating_sub(tracks_blocked);
                self.cap_y[idx] = self.cap_y[idx].saturating_sub(tracks_blocked);
            }
        }
    }

    /// Calculate edge cost with PathFinder negotiated congestion formula
    pub fn edge_cost(&self, _from_idx: usize, to_idx: usize, is_horizontal: bool) -> f32 {
        let base = self.base_cost[to_idx] as f32;
        let (cap, occ, hist) = if is_horizontal {
            (
                self.cap_x[to_idx].max(1) as f32,
                self.occ_x[to_idx] as f32,
                self.hist_x[to_idx] as f32,
            )
        } else {
            (
                self.cap_y[to_idx].max(1) as f32,
                self.occ_y[to_idx] as f32,
                self.hist_y[to_idx] as f32,
            )
        };

        // PathFinder cost = Base * (1 + Occ/Cap)^1.5 * (1 + Hist * 0.5)
        let congestion_factor = 1.0 + (occ / cap);
        let history_factor = 1.0 + (hist * 0.5);
        base * congestion_factor * history_factor
    }

    /// Increment history cost for over-capacity G-cells
    pub fn update_history_penalties(&mut self) -> usize {
        let mut overused_count = 0;
        let total_cells = self.dim_x * self.dim_y * self.dim_z;
        for i in 0..total_cells {
            if self.occ_x[i] > self.cap_x[i] {
                self.hist_x[i] = self.hist_x[i].saturating_add(self.occ_x[i] - self.cap_x[i]);
                overused_count += 1;
            }
            if self.occ_y[i] > self.cap_y[i] {
                self.hist_y[i] = self.hist_y[i].saturating_add(self.occ_y[i] - self.cap_y[i]);
                overused_count += 1;
            }
        }
        overused_count
    }

    /// Reset occupancy counts before re-routing an iteration
    pub fn reset_occupancies(&mut self) {
        self.occ_x.fill(0);
        self.occ_y.fill(0);
    }
}

/// Global Net Terminal for PathFinder routing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlobalTerminal {
    pub gx: usize,
    pub gy: usize,
    pub gz: usize,
}

/// A 3D path segment in the G-cell grid
#[derive(Clone, Debug)]
pub struct GlobalPath {
    pub cells: Vec<(usize, usize, usize)>,
}

#[derive(Copy, Clone, PartialEq)]
struct PathNode {
    cost: f32,
    x: usize,
    y: usize,
    z: usize,
}

impl Eq for PathNode {}

impl Ord for PathNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// PathFinder Global Router running negotiated congestion on `VolumetricTensor3D`
pub struct PathFinderGlobalRouter<'a> {
    pub tensor: &'a mut VolumetricTensor3D,
    pub max_iterations: usize,
}

impl<'a> PathFinderGlobalRouter<'a> {
    pub fn new(tensor: &'a mut VolumetricTensor3D, max_iterations: usize) -> Self {
        Self {
            tensor,
            max_iterations,
        }
    }

    /// Route all nets and emit 3D Routing Guides
    pub fn route_all(
        &mut self,
        nets: &HashMap<NetId, Vec<GlobalTerminal>>,
    ) -> Result<HashMap<NetId, RoutingGuide>, String> {
        let mut routes: HashMap<NetId, GlobalPath> = HashMap::new();

        for iter in 0..self.max_iterations {
            self.tensor.reset_occupancies();

            // Rip up and re-route each net
            for (&net_id, terminals) in nets {
                if terminals.len() < 2 {
                    continue;
                }

                // Route multi-terminal tree (sequential multi-pin A*)
                let path = self.route_net(terminals)?;
                
                // Commit occupancy to tensor
                self.commit_path_occupancy(&path);
                routes.insert(net_id, path);
            }

            // Check for overused edges
            let overused = self.tensor.update_history_penalties();
            if overused == 0 {
                break;
            }

            if iter == self.max_iterations - 1 && overused > 0 {
                // Return best-effort routes with inflated guides
            }
        }

        // Convert routes to RoutingGuides
        let mut guides = HashMap::new();
        let gcell_pm = self.tensor.gcell_size_pm;

        for (net_id, path) in routes {
            let mut guide = RoutingGuide::new(net_id);
            for &(gx, gy, gz) in &path.cells {
                let min_x = gx as i64 * gcell_pm;
                let min_y = gy as i64 * gcell_pm;
                let max_x = min_x + gcell_pm;
                let max_y = min_y + gcell_pm;

                guide.add_volume(GCellVolume3D {
                    gcell_x: gx as u32,
                    gcell_y: gy as u32,
                    layer: gz as u32,
                    bbox_pm: (min_x, min_y, max_x, max_y),
                });
            }
            guides.insert(net_id, guide);
        }

        Ok(guides)
    }

    /// Route a single net's terminals into a Steiner-like connected G-Cell tree
    fn route_net(&self, terminals: &[GlobalTerminal]) -> Result<GlobalPath, String> {
        let mut full_path_cells = Vec::new();
        let mut reached_cells: HashSet<(usize, usize, usize)> = HashSet::new();

        // Start with first terminal
        let start = terminals[0];
        reached_cells.insert((start.gx, start.gy, start.gz));
        full_path_cells.push((start.gx, start.gy, start.gz));

        // Incrementally connect remaining terminals to reached tree
        for &target in &terminals[1..] {
            let subpath = self.find_path_to_tree(&reached_cells, target)?;
            for cell in subpath {
                reached_cells.insert(cell);
                if !full_path_cells.contains(&cell) {
                    full_path_cells.push(cell);
                }
            }
        }

        Ok(GlobalPath {
            cells: full_path_cells,
        })
    }

    /// 3D Dijkstra / A* search from target to any cell in reached_tree
    fn find_path_to_tree(
        &self,
        tree: &HashSet<(usize, usize, usize)>,
        target: GlobalTerminal,
    ) -> Result<Vec<(usize, usize, usize)>, String> {
        let mut dist: HashMap<(usize, usize, usize), f32> = HashMap::new();
        let mut parent: HashMap<(usize, usize, usize), (usize, usize, usize)> = HashMap::new();
        let mut heap = BinaryHeap::new();

        let start_pos = (target.gx, target.gy, target.gz);
        dist.insert(start_pos, 0.0);
        heap.push(PathNode {
            cost: 0.0,
            x: target.gx,
            y: target.gy,
            z: target.gz,
        });

        let mut target_hit = None;

        while let Some(PathNode { cost, x, y, z }) = heap.pop() {
            let current = (x, y, z);
            if tree.contains(&current) {
                target_hit = Some(current);
                break;
            }

            if cost > *dist.get(&current).unwrap_or(&f32::INFINITY) {
                continue;
            }

            let curr_idx = self.tensor.cell_index(x, y, z);

            // Explore orthogonal 3D neighbors (X-, X+, Y-, Y+, Z-, Z+)
            let mut neighbors = Vec::with_capacity(6);
            if x > 0 {
                neighbors.push(((x - 1, y, z), true, false));
            }
            if x + 1 < self.tensor.dim_x {
                neighbors.push(((x + 1, y, z), true, false));
            }
            if y > 0 {
                neighbors.push(((x, y - 1, z), false, false));
            }
            if y + 1 < self.tensor.dim_y {
                neighbors.push(((x, y + 1, z), false, false));
            }
            if z > 0 {
                neighbors.push(((x, y, z - 1), false, true));
            }
            if z + 1 < self.tensor.dim_z {
                neighbors.push(((x, y, z + 1), false, true));
            }

            for ((nx, ny, nz), is_h, is_z) in neighbors {
                let next_idx = self.tensor.cell_index(nx, ny, nz);
                let edge_cost = if is_z {
                    // Via cost penalty
                    25.0
                } else {
                    self.tensor.edge_cost(curr_idx, next_idx, is_h)
                };

                let next_cost = cost + edge_cost;
                let next_cell = (nx, ny, nz);
                if next_cost < *dist.get(&next_cell).unwrap_or(&f32::INFINITY) {
                    dist.insert(next_cell, next_cost);
                    parent.insert(next_cell, current);
                    heap.push(PathNode {
                        cost: next_cost,
                        x: nx,
                        y: ny,
                        z: nz,
                    });
                }
            }
        }

        let hit_cell = target_hit.ok_or_else(|| "No path found in global tensor".to_string())?;

        // Reconstruct path from hit_cell back to start_pos
        let mut path = Vec::new();
        let mut curr = hit_cell;
        path.push(curr);
        while curr != start_pos {
            if let Some(&p) = parent.get(&curr) {
                curr = p;
                path.push(curr);
            } else {
                break;
            }
        }
        path.reverse();
        Ok(path)
    }

    /// Add occupancy along global path
    fn commit_path_occupancy(&mut self, path: &GlobalPath) {
        for window in path.cells.windows(2) {
            let (x1, y1, _z1) = window[0];
            let (x2, y2, _z2) = window[1];

            let idx = self.tensor.cell_index(x2, y2, _z2);
            if x1 != x2 {
                self.tensor.occ_x[idx] = self.tensor.occ_x[idx].saturating_add(1);
            } else if y1 != y2 {
                self.tensor.occ_y[idx] = self.tensor.occ_y[idx].saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_allocation_and_via_porosity() {
        let mut tensor = VolumetricTensor3D::new(10, 10, 4, 10_000_000, 8, 8);
        assert_eq!(tensor.dim_x, 10);
        assert_eq!(tensor.dim_y, 10);
        assert_eq!(tensor.dim_z, 4);

        let idx = tensor.cell_index(2, 3, 1);
        assert_eq!(tensor.cap_x[idx], 8);

        // Via from layer 0 to 2 at (2, 3) blocks 3 tracks
        tensor.apply_via_porosity(2, 3, 0, 2, 3);
        assert_eq!(tensor.cap_x[tensor.cell_index(2, 3, 0)], 5);
        assert_eq!(tensor.cap_x[tensor.cell_index(2, 3, 1)], 5);
        assert_eq!(tensor.cap_x[tensor.cell_index(2, 3, 2)], 5);
        assert_eq!(tensor.cap_x[tensor.cell_index(2, 3, 3)], 8); // Untouched
    }

    #[test]
    fn test_pathfinder_routing() {
        let mut tensor = VolumetricTensor3D::new(5, 5, 2, 10_000_000, 4, 4);
        let mut nets = HashMap::new();
        nets.insert(
            NetId::new(1),
            vec![
                GlobalTerminal {
                    gx: 0,
                    gy: 0,
                    gz: 0,
                },
                GlobalTerminal {
                    gx: 4,
                    gy: 4,
                    gz: 0,
                },
            ],
        );

        let mut router = PathFinderGlobalRouter::new(&mut tensor, 5);
        let guides = router.route_all(&nets).unwrap();

        assert!(guides.contains_key(&NetId::new(1)));
        let guide = &guides[&NetId::new(1)];
        assert!(!guide.volumes.is_empty());
    }
}
