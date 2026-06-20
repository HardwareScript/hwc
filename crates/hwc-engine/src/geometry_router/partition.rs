//! Partition Stage: Divides the routing space into coarse G-cells and
//! pre-negotiates net crossings via boundary ports.
//!
//! This is the global planning phase of the routing pipeline. It converts
//! the continuous routing space into a discrete grid of G-cells, identifies
//! which nets pass through each cell, and reserves boundary ports at cell
//! interfaces.

use crate::geometry::{BoundingBox, Point3D};
use crate::netlist::NetId;

/// A unique identifier for a G-cell in the partition grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GCellId(pub u32);

/// A single G-cell in the coarse partition grid.
#[derive(Clone, Debug)]
pub struct GCell {
    pub id: GCellId,
    pub bounds: BoundingBox,
    /// Nets that pass through or originate in this G-cell.
    pub nets: Vec<NetId>,
    /// Reserved boundary ports for nets crossing into adjacent G-cells.
    pub boundary_ports: Vec<BoundaryPort>,
}

/// A reserved interface port at a G-cell boundary.
///
/// When a net crosses from G-cell A to G-cell B, a `BoundaryPort` is locked
/// at the crossing coordinate with a clearance envelope.
#[derive(Clone, Debug)]
pub struct BoundaryPort {
    pub net_id: NetId,
    /// The exact coordinate where the net crosses the G-cell boundary.
    pub position: Point3D,
    /// The two G-cells that share this boundary.
    pub adjacent_cells: (GCellId, GCellId),
    /// Minimum clearance envelope around the port (C_clearance).
    pub clearance_nm: i64,
    /// Whether this port has been relocated from its original position.
    pub relocated: bool,
}

/// The coarse partition grid dividing the routing space into G-cells.
#[derive(Clone, Debug)]
pub struct PartitionGrid {
    /// G-cell dimensions in nanometers.
    pub cell_width_nm: i64,
    pub cell_height_nm: i64,
    /// Grid dimensions (columns x rows).
    pub cols: usize,
    pub rows: usize,
    /// The G-cells stored in row-major order.
    pub cells: Vec<GCell>,
    /// Board bounding box.
    pub board_bounds: BoundingBox,
    /// Track pitch in nanometers (used for boundary port relocation).
    pub track_pitch_nm: i64,
    /// Maximum clearance limit (for boundary halo expansion).
    pub max_clearance_nm: i64,
}

impl PartitionGrid {
    /// Create a new partition grid from board bounds and cell size.
    ///
    /// Divides the board into uniform G-cells of the specified size.
    pub fn new(
        board_bounds: BoundingBox,
        cell_width_nm: i64,
        cell_height_nm: i64,
        track_pitch_nm: i64,
        max_clearance_nm: i64,
    ) -> Self {
        let board_width = board_bounds.max.x - board_bounds.min.x;
        let board_height = board_bounds.max.y - board_bounds.min.y;

        let cols = ((board_width + cell_width_nm - 1) / cell_width_nm) as usize;
        let rows = ((board_height + cell_height_nm - 1) / cell_height_nm) as usize;

        let mut cells = Vec::with_capacity(cols * rows);
        for row in 0..rows {
            for col in 0..cols {
                let min_x = board_bounds.min.x + (col as i64) * cell_width_nm;
                let min_y = board_bounds.min.y + (row as i64) * cell_height_nm;
                let max_x = (min_x + cell_width_nm).min(board_bounds.max.x);
                let max_y = (min_y + cell_height_nm).min(board_bounds.max.y);

                let cell_id = GCellId((row * cols + col) as u32);
                cells.push(GCell {
                    id: cell_id,
                    bounds: BoundingBox::new(
                        Point3D::new(min_x, min_y, board_bounds.min.z),
                        Point3D::new(max_x, max_y, board_bounds.max.z),
                    ),
                    nets: Vec::new(),
                    boundary_ports: Vec::new(),
                });
            }
        }

        Self {
            cell_width_nm,
            cell_height_nm,
            cols,
            rows,
            cells,
            board_bounds,
            track_pitch_nm,
            max_clearance_nm,
        }
    }

    /// Get the G-cell containing a given point.
    #[inline]
    pub fn cell_at(&self, point: Point3D) -> Option<GCellId> {
        let dx = point.x - self.board_bounds.min.x;
        let dy = point.y - self.board_bounds.min.y;

        if dx < 0 || dy < 0 {
            return None;
        }

        let col = (dx / self.cell_width_nm) as usize;
        let row = (dy / self.cell_height_nm) as usize;

        if col >= self.cols || row >= self.rows {
            return None;
        }

        Some(GCellId((row * self.cols + col) as u32))
    }

    /// Get the G-cell by (column, row) index.
    #[inline]
    pub fn cell_at_index(&self, col: usize, row: usize) -> Option<&GCell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells.get(row * self.cols + col)
    }

    /// Get the G-cell by GCellId.
    #[inline]
    pub fn get_cell(&self, id: GCellId) -> Option<&GCell> {
        self.cells.get(id.0 as usize)
    }

    /// Get mutable G-cell by GCellId.
    #[inline]
    pub fn get_cell_mut(&mut self, id: GCellId) -> Option<&mut GCell> {
        self.cells.get_mut(id.0 as usize)
    }

    /// Get adjacent G-cell IDs for a given cell (4-connected: N, S, E, W).
    pub fn neighbors(&self, id: GCellId) -> Vec<GCellId> {
        let idx = id.0 as usize;
        let col = idx % self.cols;
        let row = idx / self.cols;

        let mut result = Vec::with_capacity(4);

        // North (row - 1)
        if row > 0 {
            result.push(GCellId(((row - 1) * self.cols + col) as u32));
        }
        // South (row + 1)
        if row + 1 < self.rows {
            result.push(GCellId(((row + 1) * self.cols + col) as u32));
        }
        // West (col - 1)
        if col > 0 {
            result.push(GCellId((row * self.cols + (col - 1)) as u32));
        }
        // East (col + 1)
        if col + 1 < self.cols {
            result.push(GCellId((row * self.cols + (col + 1)) as u32));
        }

        result
    }

    /// Register a net as passing through a G-cell.
    #[inline]
    pub fn register_net_in_cell(&mut self, cell_id: GCellId, net_id: NetId) {
        if let Some(cell) = self.get_cell_mut(cell_id) {
            if !cell.nets.contains(&net_id) {
                cell.nets.push(net_id);
            }
        }
    }

    /// Allocate boundary ports for a net crossing from one cell to another.
    ///
    /// Returns the allocated port position (may differ from requested if
    /// relocation is needed).
    pub fn allocate_boundary_port(
        &mut self,
        from: GCellId,
        to: GCellId,
        net_id: NetId,
        preferred_position: Point3D,
        clearance_nm: i64,
    ) -> Option<BoundaryPort> {
        let from_cell = self.get_cell(from)?;
        let to_cell = self.get_cell(to)?;

        let boundary = shared_boundary_bounds(from_cell, to_cell)?;

        // Clamp preferred_position to within the shared boundary
        let clamped_x = preferred_position.x
            .max(boundary.min.x)
            .min(boundary.max.x);
        let clamped_y = preferred_position.y
            .max(boundary.min.y)
            .min(boundary.max.y);
        let z = boundary.min.z;

        let position = Point3D::new(clamped_x, clamped_y, z);

        let port = BoundaryPort {
            net_id,
            position,
            adjacent_cells: (from, to),
            clearance_nm,
            relocated: false,
        };

        // Register in both cells
        if let Some(cell) = self.get_cell_mut(from) {
            cell.boundary_ports.push(port.clone());
        }
        if let Some(cell) = self.get_cell_mut(to) {
            cell.boundary_ports.push(port.clone());
        }

        Some(port)
    }

    /// Attempt localized boundary port relocation.
    ///
    /// Shifts the port +/- 3 * track_pitch along the shared boundary.
    /// Returns true if relocation succeeded within the allowed window.
    pub fn relocate_boundary_port(
        &mut self,
        port_index: usize,
        cell_id: GCellId,
    ) -> bool {
        let cell = match self.get_cell(cell_id) {
            Some(c) => c,
            None => return false,
        };

        let port = match cell.boundary_ports.get(port_index) {
            Some(p) => p.clone(),
            None => return false,
        };

        let (cell_a_id, cell_b_id) = port.adjacent_cells;
        let cell_a = match self.get_cell(cell_a_id) {
            Some(c) => c,
            None => return false,
        };
        let cell_b = match self.get_cell(cell_b_id) {
            Some(c) => c,
            None => return false,
        };

        let boundary = match shared_boundary_bounds(cell_a, cell_b) {
            Some(b) => b,
            None => return false,
        };

        let shift = 3 * self.track_pitch_nm;

        // Determine shift direction based on boundary orientation
        let boundary_width = boundary.max.x - boundary.min.x;
        let boundary_height = boundary.max.y - boundary.min.y;

        let new_position = if boundary_width > boundary_height {
            // Vertical boundary (shared X edge) -> shift along Y
            let y_options = [
                port.position.y + shift,
                port.position.y - shift,
            ];
            let mut best = None;
            for y in y_options {
                if y >= boundary.min.y && y <= boundary.max.y {
                    best = Some(Point3D::new(port.position.x, y, port.position.z));
                    break;
                }
            }
            match best {
                Some(p) => p,
                None => return false,
            }
        } else {
            // Horizontal boundary (shared Y edge) -> shift along X
            let x_options = [
                port.position.x + shift,
                port.position.x - shift,
            ];
            let mut best = None;
            for x in x_options {
                if x >= boundary.min.x && x <= boundary.max.x {
                    best = Some(Point3D::new(x, port.position.y, port.position.z));
                    break;
                }
            }
            match best {
                Some(p) => p,
                None => return false,
            }
        };

        // Update the port in both cells
        let new_port = BoundaryPort {
            position: new_position,
            relocated: true,
            ..port
        };

        if let Some(cell) = self.get_cell_mut(cell_a_id) {
            if let Some(p) = cell.boundary_ports.get_mut(port_index) {
                *p = new_port.clone();
            }
        }
        if let Some(cell) = self.get_cell_mut(cell_b_id) {
            if let Some(p) = cell.boundary_ports.get_mut(port_index) {
                *p = new_port;
            }
        }

        true
    }

    /// Get total number of G-cells.
    #[inline]
    pub fn total_cells(&self) -> usize {
        self.cols * self.rows
    }
}

impl Default for PartitionGrid {
    fn default() -> Self {
        Self::new(
            BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(100_000_000, 100_000_000, 0)),
            10_000_000, // 10mm default cell width
            10_000_000, // 10mm default cell height
            100_000,    // 0.1mm track pitch
            200_000,    // 0.2mm max clearance
        )
    }
}

/// Compute the bounding box shared between two adjacent G-cells.
///
/// This is the boundary line/area where ports can be placed. Returns `None`
/// if the cells are not adjacent (no shared edge).
pub fn shared_boundary_bounds(a: &GCell, b: &GCell) -> Option<BoundingBox> {
    let min_x = a.bounds.min.x.max(b.bounds.min.x);
    let max_x = a.bounds.max.x.min(b.bounds.max.x);
    let min_y = a.bounds.min.y.max(b.bounds.min.y);
    let max_y = a.bounds.max.y.min(b.bounds.max.y);
    let min_z = a.bounds.min.z.max(b.bounds.min.z);
    let max_z = a.bounds.max.z.min(b.bounds.max.z);

    // Adjacent cells share a boundary: intersection should be a line or point
    // (zero area) but we return the bounding box of the shared edge
    if min_x < max_x && min_y <= max_y && min_z <= max_z {
        // Vertical boundary (shared X edge): intersection has width > 0 only if cells overlap
        // For a proper partition grid, adjacent cells share an edge (zero width)
        // so min_x == max_x for vertical boundaries
        Some(BoundingBox::new(
            Point3D::new(min_x, min_y, min_z),
            Point3D::new(max_x, max_y, max_z),
        ))
    } else if min_x <= max_x && min_y < max_y && min_z <= max_z {
        Some(BoundingBox::new(
            Point3D::new(min_x, min_y, min_z),
            Point3D::new(max_x, max_y, max_z),
        ))
    } else if min_x == max_x && min_y == max_y && min_z <= max_z {
        // Corner-adjacent: they share a point but not an edge
        None
    } else {
        // No intersection
        None
    }
}

/// Partition a list of net bounding boxes into the grid.
///
/// For each net, finds all G-cells its bounding box overlaps and registers
/// the net in those cells.
pub fn partition_nets(grid: &mut PartitionGrid, net_bboxes: &[(NetId, BoundingBox)]) {
    for &(net_id, ref bbox) in net_bboxes {
        // Find the range of cells this bounding box could overlap
        let min_col = ((bbox.min.x - grid.board_bounds.min.x) / grid.cell_width_nm).max(0) as usize;
        let max_col = ((bbox.max.x - grid.board_bounds.min.x) / grid.cell_width_nm).min((grid.cols - 1) as i64) as usize;
        let min_row = ((bbox.min.y - grid.board_bounds.min.y) / grid.cell_height_nm).max(0) as usize;
        let max_row = ((bbox.max.y - grid.board_bounds.min.y) / grid.cell_height_nm).min((grid.rows - 1) as i64) as usize;

        for row in min_row..=max_row {
            for col in min_col..=max_col {
                let cell_id = GCellId((row * grid.cols + col) as u32);
                grid.register_net_in_cell(cell_id, net_id);
            }
        }
    }
}
