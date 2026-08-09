//! Navigable Space Extraction (v0.1.9 Phase 5)
//!
//! Convex-preserving trapezoidal decomposition for complex obstacle avoidance.
//! Implements the Configuration Space (C-Space) approach where obstacles are
//! inflated by `(trace_width / 2) + clearance` before decomposition.
//!
//! ## Architecture
//! 1. Minkowski Pre-Inflation: Inflate obstacles to create C-Space
//! 2. Trapezoidal Slicing: Decompose free space into navigable cells
//! 3. Convex Merging: Merge adjacent convex cells
//! 4. Adjacency Graph: Build graph for BFS/Dijkstra corridor search
//!
//! **Key Principle**: Any coordinate inside C-Space cells is guaranteed to be
//! 100% physically legal for the trace's centerline.

use crate::geometry::{BoundingBox, Point3D};
use hwc_types::Technology;

/// Errors that can occur during spatial decomposition.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SpatialDecompositionError {
    #[error("trace_width_nm must be positive, got {0}")]
    InvalidTraceWidth(i64),

    #[error("No navigable corridor found between ({start_x}, {start_y}, {start_z}) and ({end_x}, {end_y}, {end_z})")]
    NoCorridorFound {
        start_x: i64,
        start_y: i64,
        start_z: i64,
        end_x: i64,
        end_y: i64,
        end_z: i64,
    },

    #[error("Start point ({x}, {y}, {z}) is not inside any navigable cell")]
    StartPointOutsideSpace { x: i64, y: i64, z: i64 },

    #[error("End point ({x}, {y}, {z}) is not inside any navigable cell")]
    EndPointOutsideSpace { x: i64, y: i64, z: i64 },

    #[error("Board bounds have zero or negative area")]
    InvalidBoardBounds,
}

/// A navigable cell in the decomposed free space.
#[derive(Debug, Clone)]
pub struct FreeCell {
    /// Bounding box of the cell.
    pub bbox: BoundingBox,
    /// Adjacent cell indices (for graph traversal).
    pub neighbors: Vec<usize>,
    /// Semantic cost weight (higher = prefer less).
    pub cost_weight: i64,
    /// Layer Z position.
    pub z: i64,
}

/// Spatial decomposer for C-Space navigation.
#[derive(Debug)]
pub struct SpatialDecomposer {
    /// Inflated obstacles (C-Space).
    inflated_obstacles: Vec<BoundingBox>,
    /// Original obstacles (for reference).
    raw_obstacles: Vec<BoundingBox>,
    /// Trace width in nanometers.
    pub trace_width_nm: i64,
    /// Minimum clearance in nanometers.
    pub min_clearance_nm: i64,
}

impl SpatialDecomposer {
    /// Create a new spatial decomposer.
    ///
    /// # Arguments
    /// * `raw_obstacles` - Original obstacle bounding boxes
    /// * `trace_width_nm` - Width of the routing trace
    /// * `min_clearance_nm` - Minimum clearance from obstacles
    /// * `technology_strategy` - Technology strategy (PCB or ASIC)
    pub fn new(
        raw_obstacles: Vec<BoundingBox>,
        trace_width_nm: i64,
        min_clearance_nm: i64,
        technology_strategy: Technology,
    ) -> Result<Self, SpatialDecompositionError> {
        if trace_width_nm <= 0 {
            return Err(SpatialDecompositionError::InvalidTraceWidth(trace_width_nm));
        }

        let inflation = technology_strategy.obstacle_inflation(trace_width_nm, min_clearance_nm);

        eprintln!("[NAVIGABLE SPACE] Creating spatial decomposer:");
        eprintln!("  trace_width_nm = {}", trace_width_nm);
        eprintln!("  min_clearance_nm = {}", min_clearance_nm);
        eprintln!("  technology = {}", technology_strategy.name());
        eprintln!("  calculated inflation = {} nm", inflation);
        eprintln!("  raw obstacles count = {}", raw_obstacles.len());

        let inflated_obstacles: Vec<BoundingBox> = raw_obstacles
            .iter()
            .enumerate()
            .map(|(i, obs)| {
                let inflated = obs.expand(inflation);
                eprintln!(
                    "  [Obstacle {}] BEFORE inflation: ({},{},{}) to ({},{},{})",
                    i, obs.min.x, obs.min.y, obs.min.z, obs.max.x, obs.max.y, obs.max.z
                );
                eprintln!(
                    "  [Obstacle {}] AFTER inflation: ({},{},{}) to ({},{},{})",
                    i,
                    inflated.min.x,
                    inflated.min.y,
                    inflated.min.z,
                    inflated.max.x,
                    inflated.max.y,
                    inflated.max.z
                );
                inflated
            })
            .collect();

        Ok(Self {
            inflated_obstacles,
            raw_obstacles,
            trace_width_nm,
            min_clearance_nm,
        })
    }

    /// Decompose the C-Space into navigable cells.
    ///
    /// Returns a list of `FreeCell` objects that represent safe navigation regions.
    /// Any centerline routed through these cells is guaranteed to be physically legal.
    pub fn decompose(&self, board_bounds: &BoundingBox, z: i64) -> Vec<FreeCell> {
        // STEP 2: Extract X-boundaries from inflated obstacles
        let mut x_splits: Vec<i64> = Vec::new();
        x_splits.push(board_bounds.min.x);
        x_splits.push(board_bounds.max.x);

        for obs in &self.inflated_obstacles {
            if obs.min.x > board_bounds.min.x && obs.min.x < board_bounds.max.x {
                x_splits.push(obs.min.x);
            }
            if obs.max.x > board_bounds.min.x && obs.max.x < board_bounds.max.x {
                x_splits.push(obs.max.x);
            }
        }

        x_splits.sort_unstable();
        x_splits.dedup();

        // STEP 3: Create vertical slices and merge convex cells
        let mut cells = Vec::new();

        for window in x_splits.windows(2) {
            let x_min = window[0];
            let x_max = window[1];

            // Find Y-boundaries within this X-slice
            let mut y_splits: Vec<i64> = Vec::new();
            y_splits.push(board_bounds.min.y);
            y_splits.push(board_bounds.max.y);

            for obs in &self.inflated_obstacles {
                // Check if obstacle overlaps this X-slice
                if obs.min.x < x_max && obs.max.x > x_min {
                    if obs.min.y > board_bounds.min.y && obs.min.y < board_bounds.max.y {
                        y_splits.push(obs.min.y);
                    }
                    if obs.max.y > board_bounds.min.y && obs.max.y < board_bounds.max.y {
                        y_splits.push(obs.max.y);
                    }
                }
            }

            y_splits.sort_unstable();
            y_splits.dedup();

            // Create cells for each Y-interval
            for y_window in y_splits.windows(2) {
                let y_min = y_window[0];
                let y_max = y_window[1];

                let cell_bbox =
                    BoundingBox::new(Point3D::new(x_min, y_min, z), Point3D::new(x_max, y_max, z));

                // Check if this cell overlaps any inflated obstacle
                if !self.cell_overlaps_obstacle(&cell_bbox) {
                    cells.push(FreeCell {
                        bbox: cell_bbox,
                        neighbors: Vec::new(),
                        cost_weight: 1,
                        z,
                    });
                }
            }
        }

        // STEP 4: Build adjacency graph
        self.build_adjacency(&mut cells);

        cells
    }

    /// Check if a cell overlaps any inflated obstacle.
    fn cell_overlaps_obstacle(&self, cell: &BoundingBox) -> bool {
        for obs in &self.inflated_obstacles {
            if cell.intersects(obs) {
                return true;
            }
        }
        false
    }

    /// Build adjacency graph between cells.
    fn build_adjacency(&self, cells: &mut [FreeCell]) {
        let n = cells.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if self.cells_adjacent(&cells[i].bbox, &cells[j].bbox) {
                    cells[i].neighbors.push(j);
                    cells[j].neighbors.push(i);
                }
            }
        }
    }

    /// Check if two cells are adjacent (share an edge).
    fn cells_adjacent(&self, a: &BoundingBox, b: &BoundingBox) -> bool {
        // Cells are adjacent if they share an edge (not just a corner)
        let x_overlap = a.min.x < b.max.x && a.max.x > b.min.x;
        let y_overlap = a.min.y < b.max.y && a.max.y > b.min.y;

        // Adjacent if they overlap in one axis and touch in the other
        let x_touch = (a.max.x == b.min.x) || (a.min.x == b.max.x);
        let y_touch = (a.max.y == b.min.y) || (a.min.y == b.max.y);

        (x_overlap && y_touch) || (y_overlap && x_touch)
    }

    /// Extract a corridor between two points using BFS.
    ///
    /// Returns the corridor as a list of cell indices, or an error if no path exists.
    pub fn extract_corridor(
        &self,
        start: Point3D,
        end: Point3D,
        cells: &[FreeCell],
    ) -> Result<Vec<usize>, SpatialDecompositionError> {
        let start_cell = self.find_cell_containing(start, cells).ok_or({
            SpatialDecompositionError::StartPointOutsideSpace {
                x: start.x,
                y: start.y,
                z: start.z,
            }
        })?;
        let end_cell = self.find_cell_containing(end, cells).ok_or({
            SpatialDecompositionError::EndPointOutsideSpace {
                x: end.x,
                y: end.y,
                z: end.z,
            }
        })?;

        // BFS from start to end
        let mut visited = vec![false; cells.len()];
        let mut parent = vec![usize::MAX; cells.len()];
        let mut queue = std::collections::VecDeque::new();

        visited[start_cell] = true;
        queue.push_back(start_cell);

        while let Some(current) = queue.pop_front() {
            if current == end_cell {
                // Reconstruct path
                let mut path = Vec::new();
                let mut node = end_cell;
                while node != usize::MAX {
                    path.push(node);
                    node = parent[node];
                }
                path.reverse();
                return Ok(path);
            }

            for &neighbor in &cells[current].neighbors {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    parent[neighbor] = current;
                    queue.push_back(neighbor);
                }
            }
        }

        Err(SpatialDecompositionError::NoCorridorFound {
            start_x: start.x,
            start_y: start.y,
            start_z: start.z,
            end_x: end.x,
            end_y: end.y,
            end_z: end.z,
        })
    }

    /// Find the cell containing a point.
    fn find_cell_containing(&self, point: Point3D, cells: &[FreeCell]) -> Option<usize> {
        for (i, cell) in cells.iter().enumerate() {
            if cell.bbox.contains(point) {
                return Some(i);
            }
        }
        None
    }

    /// Convert a corridor (cell indices) to waypoints.
    pub fn corridor_to_waypoints(&self, corridor: &[usize], cells: &[FreeCell]) -> Vec<Point3D> {
        if corridor.is_empty() {
            return Vec::new();
        }

        let mut waypoints = Vec::new();

        eprintln!("[NAVIGABLE SPACE] Converting corridor to waypoints:");
        eprintln!("  corridor length = {} cells", corridor.len());

        for (i, &cell_idx) in corridor.iter().enumerate() {
            let cell = &cells[cell_idx];
            // Use center of cell as waypoint
            let center = Point3D::new(
                (cell.bbox.min.x + cell.bbox.max.x) / 2,
                (cell.bbox.min.y + cell.bbox.max.y) / 2,
                cell.z,
            );
            eprintln!(
                "  waypoint[{}] cell_bbox=({},{}) to ({},{}), center=({},{})",
                i,
                cell.bbox.min.x,
                cell.bbox.min.y,
                cell.bbox.max.x,
                cell.bbox.max.y,
                center.x,
                center.y
            );

            waypoints.push(center);
        }

        // Check minimum distance from waypoints to inflated obstacles
        eprintln!("[NAVIGABLE SPACE] Checking waypoint clearances to inflated obstacles:");
        for (w_idx, wp) in waypoints.iter().enumerate() {
            for (o_idx, obs) in self.inflated_obstacles.iter().enumerate() {
                let min_dist_x = if wp.x < obs.min.x {
                    obs.min.x - wp.x
                } else if wp.x > obs.max.x {
                    wp.x - obs.max.x
                } else {
                    0
                };

                let min_dist_y = if wp.y < obs.min.y {
                    obs.min.y - wp.y
                } else if wp.y > obs.max.y {
                    wp.y - obs.max.y
                } else {
                    0
                };

                let dist = if min_dist_x > 0 && min_dist_y > 0 {
                    // Point is outside in both axes - use Euclidean distance to corner
                    ((min_dist_x * min_dist_x + min_dist_y * min_dist_y) as f64).sqrt() as i64
                } else {
                    // Point is aligned with obstacle in at least one axis
                    min_dist_x.max(min_dist_y)
                };

                eprintln!(
                    "  waypoint[{}]=({},{}) to inflated_obs[{}]: distance={} nm",
                    w_idx, wp.x, wp.y, o_idx, dist
                );

                if dist < (self.trace_width_nm / 2) {
                    eprintln!(
                        "  ⚠️ WARNING: waypoint[{}] distance {} is less than trace radius {} nm!",
                        w_idx,
                        dist,
                        self.trace_width_nm / 2
                    );
                }
            }
        }

        waypoints
    }

    /// Get the inflated obstacles (for debugging/visualization).
    pub fn inflated_obstacles(&self) -> &[BoundingBox] {
        &self.inflated_obstacles
    }

    /// Get the raw obstacles (for reference).
    pub fn raw_obstacles(&self) -> &[BoundingBox] {
        &self.raw_obstacles
    }

    /// Validate that a cell is wide enough for the trace + clearance.
    ///
    /// Returns the available width in nanometers, or None if the cell is too narrow.
    pub fn validate_cell_width(&self, cell: &FreeCell) -> Option<i64> {
        let required_width = self.trace_width_nm + (2 * self.min_clearance_nm);
        let cell_width_x = cell.bbox.max.x - cell.bbox.min.x;
        let cell_width_y = cell.bbox.max.y - cell.bbox.min.y;

        // Cell must be wide enough in at least one dimension
        let available_width = cell_width_x.min(cell_width_y);
        if available_width >= required_width {
            Some(available_width)
        } else {
            None
        }
    }

    /// Get the minimum corridor width for a set of cells.
    ///
    /// Returns the bottleneck width in nanometers.
    pub fn corridor_width(&self, corridor: &[usize], cells: &[FreeCell]) -> i64 {
        corridor
            .iter()
            .filter_map(|&idx| self.validate_cell_width(&cells[idx]))
            .min()
            .unwrap_or(0)
    }

    /// Check if a corridor is wide enough for the trace.
    pub fn is_corridor_sufficient(&self, corridor: &[usize], cells: &[FreeCell]) -> bool {
        let required_width = self.trace_width_nm + (2 * self.min_clearance_nm);
        self.corridor_width(corridor, cells) >= required_width
    }
}

/// Semantic cost type for corridor search.
///
/// Users declare the cost weight directly.
/// No hardcoded multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticCost {
    /// The cost weight for this region type.
    pub weight: i64,
}

impl SemanticCost {
    /// Create a new semantic cost with the user-declared weight.
    pub fn new(weight: i64) -> Self {
        Self { weight }
    }
}
