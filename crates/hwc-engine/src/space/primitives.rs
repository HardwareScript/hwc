/// Physical dimensions in nanometers (fixed-point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width_nm: i64,
    pub height_nm: i64,
    pub depth_nm: i64,
}

impl Dimensions {
    /// Create dimensions from millimeters.
    pub fn from_mm(width_mm: f64, height_mm: f64, depth_mm: f64) -> Self {
        Self {
            width_nm: (width_mm * 1_000_000.0) as i64,
            height_nm: (height_mm * 1_000_000.0) as i64,
            depth_nm: (depth_mm * 1_000_000.0) as i64,
        }
    }

    /// Convert to millimeters.
    pub fn to_mm(&self) -> (f64, f64, f64) {
        (
            self.width_nm as f64 / 1_000_000.0,
            self.height_nm as f64 / 1_000_000.0,
            self.depth_nm as f64 / 1_000_000.0,
        )
    }
}

/// Grid cell counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCells {
    pub x_cols: usize,
    pub y_rows: usize,
    pub z_layers: usize,
}

impl GridCells {
    pub fn new(x_cols: usize, y_rows: usize, z_layers: usize) -> Self {
        Self {
            x_cols,
            y_rows,
            z_layers,
        }
    }

    pub fn total_cells(&self) -> usize {
        self.x_cols * self.y_rows * self.z_layers
    }
}

/// Voxel size in nanometers (fixed-point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoxelSize {
    pub x_nm: i64,
    pub y_nm: i64,
    pub z_nm: i64,
}

impl VoxelSize {
    /// Calculate voxel size from dimensions and grid.
    pub fn from_dimensions(dimensions: Dimensions, grid: GridCells) -> Self {
        Self {
            x_nm: dimensions.width_nm / grid.x_cols as i64,
            y_nm: dimensions.height_nm / grid.y_rows as i64,
            z_nm: dimensions.depth_nm / grid.z_layers as i64,
        }
    }
}
