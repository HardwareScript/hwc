// Thermal Relief Generation — v0.1.8 NATIVE VECTOR
// Reference: GAP1 Section 5.5
//
// Generates thermal relief patterns (spokes) for pads connected to copper pours.
// Prevents cold solder joints by restricting heat flow during manufacturing.
//
// v0.1.8: Scanline rasterization (`PolygonRasterizer`) has been deleted.
// The old approach filled polygons by scanline-rasterizing them into individual
// occupied grid points via `occupy_point()`. This produced thousands of
// degenerate zero-length TraceSegments and was incompatible with Clipper2
// boolean operations needed for pour merging.
//
// REPLACEMENT: Spokes are now registered as native vector polygons via
// `entity_graph.add_polygon_substrate_layer()`. This stores the spoke as a
// single Clipper2 Path64 polygon in `SubstrateLayerShape::Polygon`, enabling
// seamless union/intersection with copper pours and proper DRC.

use crate::geometry_router::EntityGraph;
use crate::geometry_router::substrate_types::{MaterialId, NetId};
use crate::geometry::{BoundingBox, Point2D, Point3D, Polygon};
use std::f64::consts::PI;

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

    /// Generate thermal relief pattern for a circular pad — v0.1.8 NATIVE VECTOR
    ///
    /// Writes directly to EntityGraph. No intermediate HashMap allocations.
    ///
    /// # Arguments
    /// * `center` - Pad center in nanometers
    /// * `pad_radius_nm` - Pad radius in nanometers
    /// * `z_layer` - Z coordinate in nanometers
    /// * `material` - Material ID for spokes
    /// * `net` - Net ID for spokes
    /// * `grid` - EntityGraph to write into
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
                // No relief — pad directly connects to pour (no modifications)
            }
            ThermalReliefType::Isolated => {
                // Complete isolation — drill clearance gap through substrate layers
                let clearance_radius = pad_radius_nm + self.config.gap_width_nm;
                let half = clearance_radius;
                let z_half = self.resolution_nm * 2;
                let cutout = BoundingBox::new(
                    Point3D::new(center.x - half, center.y - half, z_layer - z_half),
                    Point3D::new(center.x + half, center.y + half, z_layer + z_half),
                );
                grid.drill_hole(cutout, Some(clearance_radius * 2), net);
            }
            ThermalReliefType::Spokes => {
                self.generate_spoke_pattern(center, pad_radius_nm, z_layer, material, net, grid)
            }
        }
    }

    /// Generate spoke pattern for thermal relief — v0.1.8 NATIVE VECTOR
    ///
    /// Writes directly to EntityGraph. No intermediate HashMap allocations.
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
        let cutout = BoundingBox::new(
            Point3D::new(center.x - half, center.y - half, z_layer - z_half),
            Point3D::new(center.x + half, center.y + half, z_layer + z_half),
        );
        grid.drill_hole(cutout, Some(clearance_radius * 2), net);

        // Add spokes
        let spoke_length = self.config.gap_width_nm + self.resolution_nm * 2;
        let angle_step = 2.0 * PI / self.config.spoke_count as f64;

        for i in 0..self.config.spoke_count {
            let angle = i as f64 * angle_step;
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

    /// Generate a single spoke at given angle — v0.1.8 NATIVE VECTOR
    ///
    /// v0.1.8: Replaced `PolygonRasterizer::rasterize_into_grid()` with native
    /// polygon registration via `add_polygon_substrate_layer()`. The spoke is
    /// stored as a single Clipper2 Path64 polygon in SubstrateLayerShape::Polygon,
    /// enabling seamless boolean operations with copper pours and proper DRC.
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

        // Perpendicular direction for width
        let half_width = self.config.spoke_width_nm / 2;
        let perp_cos = -sin_angle;
        let perp_sin = cos_angle;

        // Generate rectangle vertices for spoke
        let p1_x = inner_x + (half_width as f64 * perp_cos) as i64;
        let p1_y = inner_y + (half_width as f64 * perp_sin) as i64;
        let p2_x = inner_x - (half_width as f64 * perp_cos) as i64;
        let p2_y = inner_y - (half_width as f64 * perp_sin) as i64;
        let p3_x = outer_x - (half_width as f64 * perp_cos) as i64;
        let p3_y = outer_y - (half_width as f64 * perp_sin) as i64;
        let p4_x = outer_x + (half_width as f64 * perp_cos) as i64;
        let p4_y = outer_y + (half_width as f64 * perp_sin) as i64;

        // v0.1.8: Register spoke as native vector polygon (replaces scanline rasterization)
        let polygon = Polygon::new(vec![
            Point2D::new(p1_x, p1_y),
            Point2D::new(p2_x, p2_y),
            Point2D::new(p3_x, p3_y),
            Point2D::new(p4_x, p4_y),
        ]);

        let bbox = BoundingBox::new(
            Point3D::new(
                p1_x.min(p2_x).min(p3_x).min(p4_x),
                p1_y.min(p2_y).min(p3_y).min(p4_y),
                params.z_layer,
            ),
            Point3D::new(
                p1_x.max(p2_x).max(p3_x).max(p4_x),
                p1_y.max(p2_y).max(p3_y).max(p4_y),
                params.z_layer,
            ),
        );

        grid.add_polygon_substrate_layer(params.material, params.net, bbox, polygon);
    }

    /// Generate thermal relief for a rectangular pad — v0.1.8 NATIVE VECTOR
    ///
    /// Writes directly to EntityGraph. No intermediate HashMap allocations.
    pub fn generate_for_rectangular_pad(&self, params: RectangularPadParams, grid: &mut EntityGraph) {
        match self.config.relief_type {
            ThermalReliefType::Direct => {
                // No relief — pad directly connects to pour (no modifications)
            }
            ThermalReliefType::Isolated => {
                // Clear clearance around pad using drill_hole
                let gap = self.config.gap_width_nm;
                let half_w = params.width_nm / 2 + gap;
                let half_h = params.height_nm / 2 + gap;
                let z_half = self.resolution_nm * 2;
                let cutout = BoundingBox::new(
                    Point3D::new(params.center.x - half_w, params.center.y - half_h, params.z_layer - z_half),
                    Point3D::new(params.center.x + half_w, params.center.y + half_h, params.z_layer + z_half),
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
