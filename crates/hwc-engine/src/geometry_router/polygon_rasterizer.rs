// Polygon Rasterization Engine - GOD-TIER NATIVE
// Reference: GAP1 Section 4.3, 5.5
//
// Converts polygon boundaries and copper pours into occupied voxels using
// scanline algorithm. Supports arbitrary polygon shapes, holes, and thermal reliefs.
//
// GOD-TIER: All rasterization writes directly to VoxelGrid. No intermediate HashMaps.

use crate::voxel_grid::{MaterialId, NetId, VoxelGrid};
use compact_str::CompactString;
use std::cmp::{max, min};

/// Quantization error warning
#[derive(Debug, Clone, PartialEq)]
pub struct QuantizationWarning {
    /// Description of what was quantized
    pub feature: CompactString,
    /// Intended dimension in nanometers
    pub intended_nm: i64,
    /// Actual dimension after quantization in nanometers
    pub actual_nm: i64,
    /// Error percentage (0.0 to 100.0)
    pub error_percent: f64,
    /// Suggested voxel size for < 5% error
    pub suggested_voxel_size_nm: i64,
}

impl QuantizationWarning {
    /// Create a new quantization warning
    pub fn new(
        feature: CompactString,
        intended_nm: i64,
        actual_nm: i64,
        suggested_voxel_size_nm: i64,
    ) -> Self {
        let error_percent = if intended_nm > 0 {
            ((intended_nm - actual_nm).abs() as f64 / intended_nm as f64) * 100.0
        } else {
            0.0
        };

        Self {
            feature,
            intended_nm,
            actual_nm,
            error_percent,
            suggested_voxel_size_nm,
        }
    }

    /// Format warning message for display
    pub fn format_message(&self) -> CompactString {
        format!(
            "⚠️  Quantization Error: {} - Intended: {:.3}mm, Actual: {:.3}mm ({:.1}% error). \
             Suggested voxel size: {:.3}mm for < 5% error.",
            self.feature,
            self.intended_nm as f64 / 1_000_000.0,
            self.actual_nm as f64 / 1_000_000.0,
            self.error_percent,
            self.suggested_voxel_size_nm as f64 / 1_000_000.0
        )
        .into()
    }
}

/// Quantization statistics for a design
#[derive(Debug, Clone, Default)]
pub struct QuantizationStats {
    /// All warnings generated during rasterization
    pub warnings: Vec<QuantizationWarning>,
    /// Total number of features checked
    pub features_checked: usize,
    /// Number of features with > 5% error
    pub features_with_high_error: usize,
    /// Maximum error percentage encountered
    pub max_error_percent: f64,
}

/// Represents a 2D point for polygon vertices (in nanometers)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point2D {
    pub x: i64,
    pub y: i64,
}

impl Point2D {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// Represents a polygon with optional holes
#[derive(Debug, Clone)]
pub struct Polygon {
    /// Outer boundary vertices (clockwise or counter-clockwise)
    pub outer: Vec<Point2D>,
    /// Inner holes (opposite winding order from outer)
    pub holes: Vec<Vec<Point2D>>,
}

impl Polygon {
    pub fn new(outer: Vec<Point2D>) -> Self {
        Self {
            outer,
            holes: Vec::new(),
        }
    }

    pub fn with_holes(outer: Vec<Point2D>, holes: Vec<Vec<Point2D>>) -> Self {
        Self { outer, holes }
    }

    /// Calculate bounding box of the polygon
    pub fn bounding_box(&self) -> (Point2D, Point2D) {
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for point in &self.outer {
            min_x = min(min_x, point.x);
            min_y = min(min_y, point.y);
            max_x = max(max_x, point.x);
            max_y = max(max_y, point.y);
        }

        (Point2D::new(min_x, min_y), Point2D::new(max_x, max_y))
    }
}

/// Edge representation for scanline algorithm
#[derive(Debug, Clone)]
struct Edge {
    y_min: i64,
    y_max: i64,
    x_at_y_min: i64,
    inverse_slope: f64, // dx/dy
}

impl Edge {
    fn new(p1: Point2D, p2: Point2D) -> Option<Self> {
        // Skip horizontal edges
        if p1.y == p2.y {
            return None;
        }

        let (bottom, top) = if p1.y < p2.y { (p1, p2) } else { (p2, p1) };

        let dy = (top.y - bottom.y) as f64;
        let dx = (top.x - bottom.x) as f64;
        let inverse_slope = dx / dy;

        Some(Edge {
            y_min: bottom.y,
            y_max: top.y,
            x_at_y_min: bottom.x,
            inverse_slope,
        })
    }

    /// Calculate X coordinate at given Y scanline
    fn x_at_y(&self, y: i64) -> i64 {
        let dy = (y - self.y_min) as f64;
        (self.x_at_y_min as f64 + self.inverse_slope * dy).round() as i64
    }
}

/// Polygon rasterizer using scanline algorithm
///
/// GOD-TIER: Rasterizes directly into VoxelGrid for maximum performance.
/// Uses flat array indexing and bitwise operations instead of HashMap.
pub struct PolygonRasterizer {
    voxel_size_nm: i64,
    /// Quantization statistics (optional tracking)
    quantization_stats: Option<QuantizationStats>,
    /// Error threshold for warnings (default: 5%)
    error_threshold_percent: f64,
}

impl PolygonRasterizer {
    pub fn new(voxel_size_nm: i64) -> Self {
        Self {
            voxel_size_nm,
            quantization_stats: None,
            error_threshold_percent: 5.0,
        }
    }

    /// Enable quantization error tracking
    pub fn with_quantization_tracking(mut self) -> Self {
        self.quantization_stats = Some(QuantizationStats::default());
        self
    }

    /// Set error threshold for warnings (default: 5%)
    pub fn with_error_threshold(mut self, threshold_percent: f64) -> Self {
        self.error_threshold_percent = threshold_percent;
        self
    }

    /// Get quantization statistics
    pub fn quantization_stats(&self) -> Option<&QuantizationStats> {
        self.quantization_stats.as_ref()
    }

    /// Take quantization statistics (consumes the stats)
    pub fn take_quantization_stats(&mut self) -> Option<QuantizationStats> {
        self.quantization_stats.take()
    }

    /// Check quantization error for a dimension
    ///
    /// Returns a warning if error > threshold
    ///
    /// # Arguments
    /// * `feature` - Description of the feature being checked
    /// * `intended_nm` - Intended dimension in nanometers
    ///
    /// # Performance
    /// O(1) - simple arithmetic operations
    fn check_quantization_error(
        &mut self,
        feature: CompactString,
        intended_nm: i64,
    ) -> Option<QuantizationWarning> {
        // Calculate actual dimension after quantization
        let voxel_count = (intended_nm + self.voxel_size_nm / 2) / self.voxel_size_nm;
        let actual_nm = voxel_count * self.voxel_size_nm;

        // Calculate error percentage
        let error_percent = if intended_nm > 0 {
            ((intended_nm - actual_nm).abs() as f64 / intended_nm as f64) * 100.0
        } else {
            0.0
        };

        // Update stats if tracking enabled
        if let Some(stats) = &mut self.quantization_stats {
            stats.features_checked += 1;
            if error_percent > stats.max_error_percent {
                stats.max_error_percent = error_percent;
            }
            if error_percent > self.error_threshold_percent {
                stats.features_with_high_error += 1;
            }
        }

        // Generate warning if error exceeds threshold
        if error_percent > self.error_threshold_percent {
            // Calculate suggested voxel size for < 5% error
            // For 5% error: voxel_size = intended / 20
            let suggested_voxel_size_nm = (intended_nm / 20).max(1000); // At least 1μm

            let warning =
                QuantizationWarning::new(feature, intended_nm, actual_nm, suggested_voxel_size_nm);

            // Add to stats if tracking enabled
            if let Some(stats) = &mut self.quantization_stats {
                stats.warnings.push(warning.clone());
            }

            Some(warning)
        } else {
            None
        }
    }

    /// Rasterize a polygon at a specific Z layer directly into VoxelGrid
    ///
    /// GOD-TIER: No intermediate HashMap, writes directly to VoxelGrid.
    /// Uses O(1) flat array indexing instead of O(1) hash lookups.
    ///
    /// # Arguments
    /// * `polygon` - Polygon to rasterize
    /// * `z_layer` - Z coordinate in nanometers
    /// * `material` - Material ID to fill with
    /// * `net` - Net ID to assign
    /// * `grid` - VoxelGrid to write into
    ///
    /// # Quantization Checking
    /// If quantization tracking is enabled, checks polygon dimensions for precision loss
    pub fn rasterize_into_grid(
        &mut self,
        polygon: &Polygon,
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut VoxelGrid,
    ) {
        // Check quantization error for polygon dimensions
        if self.quantization_stats.is_some() {
            let (min, max) = polygon.bounding_box();
            let width = max.x - min.x;
            let height = max.y - min.y;

            self.check_quantization_error(format!("Polygon width (net {})", net).into(), width);
            self.check_quantization_error(format!("Polygon height (net {})", net).into(), height);
        }

        // Build edge table for outer boundary
        let outer_edges = self.build_edge_table(&polygon.outer);

        // Rasterize outer boundary
        self.scanline_fill_into_grid(&outer_edges, z_layer, material, net, grid, true);

        // Remove holes
        for hole in &polygon.holes {
            let hole_edges = self.build_edge_table(hole);
            self.scanline_fill_into_grid(&hole_edges, z_layer, material, net, grid, false);
        }
    }

    /// Build edge table from polygon vertices
    fn build_edge_table(&self, vertices: &[Point2D]) -> Vec<Edge> {
        let mut edges = Vec::new();

        for i in 0..vertices.len() {
            let p1 = vertices[i];
            let p2 = vertices[(i + 1) % vertices.len()];

            if let Some(edge) = Edge::new(p1, p2) {
                edges.push(edge);
            }
        }

        edges
    }

    /// Scanline fill algorithm - GOD-TIER version
    ///
    /// Writes directly to VoxelGrid using flat array indexing.
    /// No intermediate HashMap, no hash collisions.
    ///
    /// # Arguments
    /// * `edges` - Edge table from polygon
    /// * `z_layer` - Z coordinate in nanometers
    /// * `material` - Material ID to fill with
    /// * `net` - Net ID to assign
    /// * `grid` - VoxelGrid to write into
    /// * `fill` - true to fill, false to clear (for holes)
    fn scanline_fill_into_grid(
        &self,
        edges: &[Edge],
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut VoxelGrid,
        fill: bool,
    ) {
        if edges.is_empty() {
            return;
        }

        // Find Y range
        let y_min = edges.iter().map(|e| e.y_min).min().unwrap();
        let y_max = edges.iter().map(|e| e.y_max).max().unwrap();

        // Convert to voxel coordinates
        let y_min_voxel = y_min / self.voxel_size_nm;
        let y_max_voxel = y_max / self.voxel_size_nm;
        let z_voxel = (z_layer / 1_000_000).max(0) as usize; // 1mm layers

        // Scan each horizontal line
        for y_voxel in y_min_voxel..=y_max_voxel {
            let y_nm = y_voxel * self.voxel_size_nm;

            // Find all edge intersections at this Y
            let mut intersections: Vec<i64> = edges
                .iter()
                .filter(|e| e.y_min <= y_nm && y_nm < e.y_max)
                .map(|e| e.x_at_y(y_nm))
                .collect();

            // Sort intersections by X
            intersections.sort_unstable();

            // Fill between pairs of intersections (even-odd rule)
            for chunk in intersections.chunks(2) {
                if chunk.len() == 2 {
                    let x_start = (chunk[0] / self.voxel_size_nm).max(0) as usize;
                    let x_end = (chunk[1] / self.voxel_size_nm).max(0) as usize;
                    let y_voxel_usize = y_voxel.max(0) as usize;

                    for x_voxel in x_start..=x_end {
                        if fill {
                            // GOD-TIER: Direct VoxelGrid write, O(1) flat array indexing
                            grid.set_occupied(
                                x_voxel,
                                y_voxel_usize,
                                z_voxel,
                                material,
                                crate::netlist::NetHandle::new(net),
                            );
                        } else {
                            // Clear for holes
                            grid.clear(x_voxel, y_voxel_usize, z_voxel);
                        }
                    }
                }
            }
        }
    }

    /// Rasterize a rectangular pour directly into VoxelGrid (optimized path)
    ///
    /// GOD-TIER: Uses VoxelGrid::fill_box() for maximum performance.
    ///
    /// # Quantization Checking
    /// If quantization tracking is enabled, checks rectangle dimensions for precision loss
    pub fn rasterize_rectangle_into_grid(
        &mut self,
        min: Point2D,
        max: Point2D,
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut VoxelGrid,
    ) {
        // Check quantization error for rectangle dimensions
        if self.quantization_stats.is_some() {
            let width = max.x - min.x;
            let height = max.y - min.y;

            self.check_quantization_error(format!("Rectangle width (net {})", net).into(), width);
            self.check_quantization_error(format!("Rectangle height (net {})", net).into(), height);
        }

        use crate::geometry::{BoundingBox, Point3D};
        use crate::space::VoxelSize;

        let bbox = BoundingBox::new(
            Point3D::new(min.x, min.y, z_layer),
            Point3D::new(max.x, max.y, z_layer),
        );

        let voxel_size = VoxelSize {
            x_nm: self.voxel_size_nm,
            y_nm: self.voxel_size_nm,
            z_nm: 1_000_000, // 1mm layers
        };

        // GOD-TIER: Use VoxelGrid's optimized fill_box
        grid.fill_box(&bbox, &voxel_size, material, net);
    }

    /// Rasterize a circle directly into VoxelGrid (for pads and anti-pads)
    ///
    /// GOD-TIER: Writes directly to VoxelGrid using flat array indexing.
    ///
    /// # Quantization Checking
    /// If quantization tracking is enabled, checks circle diameter for precision loss
    pub fn rasterize_circle_into_grid(
        &mut self,
        center: Point2D,
        radius_nm: i64,
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut VoxelGrid,
    ) {
        // Check quantization error for circle diameter
        if self.quantization_stats.is_some() {
            let diameter = radius_nm * 2;
            self.check_quantization_error(
                format!("Circle diameter (net {})", net).into(),
                diameter,
            );
        }

        let center_x_voxel = (center.x / self.voxel_size_nm).max(0) as usize;
        let center_y_voxel = (center.y / self.voxel_size_nm).max(0) as usize;
        let radius_voxels = ((radius_nm / self.voxel_size_nm) + 1).max(0) as usize;
        let z_voxel = (z_layer / 1_000_000).max(0) as usize;

        // Use midpoint circle algorithm
        for dy in -(radius_voxels as i64)..=(radius_voxels as i64) {
            for dx in -(radius_voxels as i64)..=(radius_voxels as i64) {
                let dist_squared = dx * dx + dy * dy;
                let radius_voxels_squared = (radius_voxels as i64) * (radius_voxels as i64);

                if dist_squared <= radius_voxels_squared {
                    let x = (center_x_voxel as i64 + dx).max(0) as usize;
                    let y = (center_y_voxel as i64 + dy).max(0) as usize;

                    // GOD-TIER: Direct VoxelGrid write
                    grid.set_occupied(x, y, z_voxel, material, crate::netlist::NetHandle::new(net));
                }
            }
        }
    }
}
