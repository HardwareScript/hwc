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
