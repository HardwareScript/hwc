// Thermal Relief Generation - GOD-TIER NATIVE
// Reference: GAP1 Section 5.5
//
// Generates thermal relief patterns (spokes) for pads connected to copper pours.
// Prevents cold solder joints by restricting heat flow during manufacturing.
//
// GOD-TIER: All operations write directly to VoxelGrid. No intermediate HashMaps.

use crate::geometry_router::EntityGraph;
use crate::geometry_router::substrate_types::{MaterialId, NetId};
use crate::geometry::{Point2D, Polygon};
use std::f64::consts::PI;

/// Polygon rasterizer for filling polygons into the entity graph
pub struct PolygonRasterizer {
    #[allow(dead_code)]
    resolution_nm: i64,
}

impl PolygonRasterizer {
    pub fn new(resolution_nm: i64) -> Self {
        Self { resolution_nm }
    }

    pub fn rasterize_into_grid(
        &self,
        polygon: &Polygon,
        z_layer: i64,
        material: crate::geometry_router::substrate_types::MaterialId,
        net: crate::geometry_router::substrate_types::NetId,
        grid: &mut EntityGraph,
    ) {
        if polygon.points.len() < 3 {
            return;
        }
        let min_y = polygon.points.iter().map(|p| p.y).min().unwrap_or(0);
        let max_y = polygon.points.iter().map(|p| p.y).max().unwrap_or(0);

        let res = self.resolution_nm.max(1);
        let mut y = min_y;
        while y <= max_y {
            let mut intersections = Vec::new();
            let n = polygon.points.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let p1 = &polygon.points[i];
                let p2 = &polygon.points[j];
                if (p1.y <= y && p2.y > y) || (p2.y <= y && p1.y > y) {
                    let t = (y - p1.y) as f64 / (p2.y - p1.y) as f64;
                    let x_intersect = p1.x as f64 + t * (p2.x - p1.x) as f64;
                    intersections.push(x_intersect as i64);
                }
            }
            intersections.sort();
            let mut k = 0;
            while k + 1 < intersections.len() {
                let x_start = intersections[k];
                let x_end = intersections[k + 1];
                let mut x = x_start;
                while x <= x_end {
                    let point = crate::geometry::Point3D::new(x, y, z_layer);
                    grid.occupy_point(point, crate::netlist::NetId(net), material);
                    x += res;
                }
                k += 2;
            }
            y += res;
        }
    }
}

/// Thermal relief pattern type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalReliefType {
    /// 4 spokes at 90° intervals (most common)
    Spokes,
    /// Direct connection (for high-current pads)
    Direct,
    /// Complete isolation (for non-matching nets)
    Isolated,
}

/// Thermal relief configuration
#[derive(Debug, Clone)]
pub struct ThermalReliefConfig {
    /// Width of each spoke in nanometers
    pub spoke_width_nm: i64,
    /// Number of spokes (typically 4)
    pub spoke_count: u8,
    /// Air gap width around pad in nanometers
    pub gap_width_nm: i64,
    /// Relief type
    pub relief_type: ThermalReliefType,
}

impl Default for ThermalReliefConfig {
    fn default() -> Self {
        Self {
            spoke_width_nm: 300_000, // 0.3mm
            spoke_count: 4,
            gap_width_nm: 200_000, // 0.2mm
            relief_type: ThermalReliefType::Spokes,
        }
    }
}

/// Thermal relief generator
pub struct ThermalReliefGenerator {
    config: ThermalReliefConfig,
    resolution_nm: i64,
}

/// Parameters for spoke generation
struct SpokeParams {
    center: Point2D,
    angle_rad: f64,
    start_radius_nm: i64,
    length_nm: i64,
    z_layer: i64,
    material: MaterialId,
    net: NetId,
}

/// Parameters for rectangular pad thermal relief
pub struct RectangularPadParams {
    pub center: Point2D,
    pub width_nm: i64,
    pub height_nm: i64,
    pub z_layer: i64,
    pub material: MaterialId,
    pub net: NetId,
}

impl ThermalReliefGenerator {
    pub fn new(config: ThermalReliefConfig, resolution_nm: i64) -> Self {
        Self {
            config,
            resolution_nm,
        }
    }

    /// Generate thermal relief pattern for a circular pad - GOD-TIER NATIVE
    ///
    /// Writes directly to VoxelGrid. No intermediate HashMap allocations.
    ///
    /// # Arguments
    /// * `center` - Pad center in nanometers
    /// * `pad_radius_nm` - Pad radius in nanometers
    /// * `z_layer` - Z coordinate in nanometers
    /// * `material` - Material ID for spokes
    /// * `net` - Net ID for spokes
    /// * `grid` - VoxelGrid to write into
    pub fn generate_for_circular_pad(
        &self,
        center: Point2D,
        pad_radius_nm: i64,
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut EntityGraph,
    ) {
        match self.config.relief_type {
            ThermalReliefType::Direct => {
                // No relief - pad directly connects to pour (no modifications)
            }
            ThermalReliefType::Isolated => {
                // Complete isolation - drill clearance gap through substrate layers
                let clearance_radius = pad_radius_nm + self.config.gap_width_nm;
                let half = clearance_radius;
                let z_half = self.resolution_nm * 2;
                let cutout = crate::geometry::BoundingBox::new(
                    crate::geometry::Point3D::new(center.x - half, center.y - half, z_layer - z_half),
                    crate::geometry::Point3D::new(center.x + half, center.y + half, z_layer + z_half),
                );
                grid.drill_hole(cutout, Some(clearance_radius * 2), net);
            }
            ThermalReliefType::Spokes => {
                self.generate_spoke_pattern(center, pad_radius_nm, z_layer, material, net, grid)
            }
        }
    }

    /// Generate spoke pattern for thermal relief - GOD-TIER NATIVE
    ///
    /// Writes directly to VoxelGrid. No intermediate HashMap allocations.
    fn generate_spoke_pattern(
        &self,
        center: Point2D,
        pad_radius_nm: i64,
        z_layer: i64,
        material: MaterialId,
        net: NetId,
        grid: &mut EntityGraph,
    ) {
        // Clear clearance gap around pad
        let clearance_radius = pad_radius_nm + self.config.gap_width_nm;

        // Clear clearance gap via substrate drill_hole
        let half = clearance_radius;
        let z_half = self.resolution_nm * 2;
        let cutout = crate::geometry::BoundingBox::new(
            crate::geometry::Point3D::new(center.x - half, center.y - half, z_layer - z_half),
            crate::geometry::Point3D::new(center.x + half, center.y + half, z_layer + z_half),
        );
        grid.drill_hole(cutout, Some(clearance_radius * 2), net);

        // Add spokes
        let spoke_length = self.config.gap_width_nm + self.resolution_nm * 2;
        let angle_step = 2.0 * PI / self.config.spoke_count as f64;

        for i in 0..self.config.spoke_count {
            let angle = i as f64 * angle_step;
            // GOD-TIER: Generate spoke directly into VoxelGrid
            self.generate_spoke(
                SpokeParams {
                    center,
                    angle_rad: angle,
                    start_radius_nm: pad_radius_nm,
                    length_nm: spoke_length,
                    z_layer,
                    material,
                    net,
                },
                grid,
            );
        }
    }

    /// Generate a single spoke at given angle - GOD-TIER NATIVE
    ///
    /// Writes directly to VoxelGrid. No intermediate HashMap allocations.
    fn generate_spoke(&self, params: SpokeParams, grid: &mut EntityGraph) {
        let cos_angle = params.angle_rad.cos();
        let sin_angle = params.angle_rad.sin();

        // Calculate spoke endpoints
        let inner_x = params.center.x + (params.start_radius_nm as f64 * cos_angle) as i64;
        let inner_y = params.center.y + (params.start_radius_nm as f64 * sin_angle) as i64;
        let outer_x = params.center.x
            + ((params.start_radius_nm + params.length_nm) as f64 * cos_angle) as i64;
        let outer_y = params.center.y
            + ((params.start_radius_nm + params.length_nm) as f64 * sin_angle) as i64;

        // Rasterize spoke as a thick line
        let half_width = self.config.spoke_width_nm / 2;

        // Perpendicular direction for width
        let perp_cos = -sin_angle;
        let perp_sin = cos_angle;

        // Generate rectangle for spoke
        let p1_x = inner_x + (half_width as f64 * perp_cos) as i64;
        let p1_y = inner_y + (half_width as f64 * perp_sin) as i64;
        let p2_x = inner_x - (half_width as f64 * perp_cos) as i64;
        let p2_y = inner_y - (half_width as f64 * perp_sin) as i64;
        let p3_x = outer_x - (half_width as f64 * perp_cos) as i64;
        let p3_y = outer_y - (half_width as f64 * perp_sin) as i64;
        let p4_x = outer_x + (half_width as f64 * perp_cos) as i64;
        let p4_y = outer_y + (half_width as f64 * perp_sin) as i64;

        // GOD-TIER: Rasterize spoke rectangle directly into VoxelGrid
        let rasterizer = PolygonRasterizer::new(self.resolution_nm);
        let polygon = Polygon::new(vec![
            Point2D::new(p1_x, p1_y),
            Point2D::new(p2_x, p2_y),
            Point2D::new(p3_x, p3_y),
            Point2D::new(p4_x, p4_y),
        ]);

        rasterizer.rasterize_into_grid(&polygon, params.z_layer, params.material, params.net, grid);
    }

    /// Generate thermal relief for a rectangular pad - GOD-TIER NATIVE
    ///
    /// Writes directly to VoxelGrid. No intermediate HashMap allocations.
    pub fn generate_for_rectangular_pad(&self, params: RectangularPadParams, grid: &mut EntityGraph) {
        match self.config.relief_type {
            ThermalReliefType::Direct => {
                // No relief - pad directly connects to pour (no modifications)
            }
            ThermalReliefType::Isolated => {
                // Clear clearance around pad using drill_hole
                let gap = self.config.gap_width_nm;
                let half_w = params.width_nm / 2 + gap;
                let half_h = params.height_nm / 2 + gap;
                let z_half = self.resolution_nm * 2;
                let cutout = crate::geometry::BoundingBox::new(
                    crate::geometry::Point3D::new(params.center.x - half_w, params.center.y - half_h, params.z_layer - z_half),
                    crate::geometry::Point3D::new(params.center.x + half_w, params.center.y + half_h, params.z_layer + z_half),
                );
                grid.drill_hole(cutout, None, params.net);
            }
            ThermalReliefType::Spokes => {
                // Use approximate radius for spoke generation
                let approx_radius = (params.width_nm.min(params.height_nm)) / 2;
                self.generate_spoke_pattern(
                    params.center,
                    approx_radius,
                    params.z_layer,
                    params.material,
                    params.net,
                    grid,
                );
            }
        }
    }
}
