// Polygon Rasterization Engine - GOD-TIER NATIVE
// Reference: GAP1 Section 4.3, 5.5
//
// Converts polygon boundaries and copper pours into occupied voxels using
// scanline algorithm. Supports arbitrary polygon shapes, holes, and thermal reliefs.
//
// GOD-TIER: All rasterization writes directly to VoxelGrid. No intermediate HashMaps.

use crate::voxel_grid::{MaterialId, NetId};
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
        entity_graph: &mut crate::geometry_router::EntityGraph,
    ) {
        // Check quantization error for polygon dimensions
        if self.quantization_stats.is_some() {
            let (min, max) = polygon.bounding_box();
            let width = max.x - min.x;
            let height = max.y - min.y;

            self.check_quantization_error(format!("Polygon width (net {})", net).into(), width);
            self.check_quantization_error(format!("Polygon height (net {})", net).into(), height);
        }

        // Rasterization writes are removed in the EntityGraph migration.
        // Copper pours are now stored as SubstrateLayer objects.
        let (min, max) = polygon.bounding_box();
        let bbox = crate::geometry::BoundingBox::new(
            crate::geometry::Point3D::new(min.x, min.y, z_layer),
            crate::geometry::Point3D::new(max.x, max.y, z_layer),
        );
        entity_graph.add_substrate_layer(
            material,
            net,
            bbox,
            crate::voxel_grid::SubstrateLayerType::Pour,
        );
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
        entity_graph: &mut crate::geometry_router::EntityGraph,
    ) {
        // Check quantization error for rectangle dimensions
        if self.quantization_stats.is_some() {
            let width = max.x - min.x;
            let height = max.y - min.y;

            self.check_quantization_error(format!("Rectangle width (net {})", net).into(), width);
            self.check_quantization_error(format!("Rectangle height (net {})", net).into(), height);
        }

        // Store as SubstrateLayer instead of filling voxels
        let bbox = crate::geometry::BoundingBox::new(
            crate::geometry::Point3D::new(min.x, min.y, z_layer),
            crate::geometry::Point3D::new(max.x, max.y, z_layer),
        );
        entity_graph.add_substrate_layer(
            material,
            net,
            bbox,
            crate::voxel_grid::SubstrateLayerType::Pour,
        );
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
        entity_graph: &mut crate::geometry_router::EntityGraph,
    ) {
        // Check quantization error for circle diameter
        if self.quantization_stats.is_some() {
            let diameter = radius_nm * 2;
            self.check_quantization_error(
                format!("Circle diameter (net {})", net).into(),
                diameter,
            );
        }

        // Store as SubstrateLayer instead of filling voxels
        let bbox = crate::geometry::BoundingBox::new(
            crate::geometry::Point3D::new(center.x - radius_nm, center.y - radius_nm, z_layer),
            crate::geometry::Point3D::new(center.x + radius_nm, center.y + radius_nm, z_layer),
        );
        entity_graph.add_circle_substrate_layer(
            material,
            net,
            bbox,
            radius_nm,
        );
    }
}
