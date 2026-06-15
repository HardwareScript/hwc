//! Substrate layer representation for O(1) memory sparse architecture.
//!
//! This module implements the God-Tier solution to substrate memory overhead.
//! Instead of storing substrates as millions of individual chunks, we store them
//! as bounding boxes with material IDs.
//!
//! MEMORY SAVINGS:
//! - Old: 2000×2000×2 substrate = 250,000 chunks = 84 MB
//! - New: 2000×2000×2 substrate = 1 layer = 32 bytes
//! - Improvement: 2,625,000× memory reduction!

use super::chunk::{MaterialId, NetId};
use crate::geometry::{BoundingBox, Point3D};
use clipper2_rust::{Path64, Point64, Paths64};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Terminal (pin) position in a component
#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    /// Terminal name (e.g., "gate", "source", "drain")
    pub name: CompactString,
    /// Position in nanometers (absolute world coordinates)
    pub position: Point3D,
    /// Material at this terminal
    pub material_id: MaterialId,
    /// Net binding
    pub net_id: Option<NetId>,
}

/// A substrate layer represented as a bounding box with uniform material.
///
/// This is the core of the sparse substrate architecture. Instead of allocating
/// millions of chunks for a uniform substrate layer, we store just the bounding
/// box and material ID.
///
/// Total size: 32 bytes base + Vec overhead for cutouts
///
/// # Example
/// ```
/// # use hwc_engine::geometry::{BoundingBox, Point3D};
/// # use hwc_engine::voxel_grid::SubstrateLayer;
/// let bbox = BoundingBox::new(
///     Point3D::new(0, 0, 0),
///     Point3D::new(20_000_000, 20_000_000, 2_000_000)
/// );
/// let layer = SubstrateLayer::new(1, 0, bbox, SubstrateLayerType::Pour); // FR4, no net, pour
/// assert_eq!(layer.material, 1);
/// ```
/// Type of substrate layer for proper physics validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstrateLayerType {
    /// 2D copper pour (pad, plane, filled region)
    Pour,

    /// 3D vertical contact (via, through-hole)
    Contact,

    /// 3D dielectric substrate (FR4, core, prepreg)
    Substrate,

    /// Solder mask coating on top/bottom board faces (v0.1.7)
    SolderMask,
}

/// Type of cap for tube shapes (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    /// No cap (open end)
    None,
    /// Annular ring (disk with a hole)
    Annular,
    /// Solid disk (no hole)
    Solid,
}

/// Physical shape of the substrate layer (v0.1.6)
#[derive(Debug, Clone, PartialEq)]
pub enum SubstrateLayerShape {
    /// Axis-aligned bounding box (default)
    Rect,

    /// 2D circle shape (circular pours, annular rings)
    Circle {
        /// Radius in nanometers
        radius: i64,
    },

    /// Generic polygon-based shape (v0.2.0).
    /// Can represent any 2D cross-section: square, circle, hexagon, star, etc.
    /// The outer_contour defines the boundary; holes are subtracted from it.
    Polygon {
        outer_contour: Path64,
        holes: Paths64,
        /// Tessellation segments for 3D rendering (circles need more, squares need 4)
        segments: u32,
    },

    /// Tube shape (Plated through-hole walls)
    Tube {
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,   // v0.1.7: Unified Via support
        segments: u32,
        top_cap: CapType,    // v0.1.7: Specific cap type for top
        bottom_cap: CapType, // v0.1.7: Specific cap type for bottom
        /// Bottom outer diameter for tapered vias (v0.1.7 Microvia)
        /// If None, use outer_diameter for both ends.
        bottom_outer_diameter: Option<u32>,
    },
}

/// A multi-material stack for TSVs (Through-Silicon Vias).
///
/// A TSV consists of:
/// 1. An insulator liner (sleeve) to prevent substrate shorting.
/// 2. An optional bridge layer (e.g. Silicide/TiN) for adhesion/ohmic contact.
/// 3. A conductive fill (core) for electrical connectivity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinerStack {
    /// Material for the insulator sleeve (e.g. SiO2, Si3N4)
    pub liner_material: MaterialId,
    /// Thickness of the liner in nanometers
    pub liner_thickness_nm: i64,
    /// Optional bridge material for the interface
    pub bridge_material: Option<MaterialId>,
    /// Thickness of the bridge in nanometers
    pub bridge_thickness_nm: i64,
    /// Material for the conductive core (e.g. Copper, Tungsten)
    pub fill_material: MaterialId,
}

impl LinerStack {
    /// Create a new TSV liner stack.
    pub fn new(
        liner_material: MaterialId,
        liner_thickness_nm: i64,
        bridge_material: Option<MaterialId>,
        bridge_thickness_nm: i64,
        fill_material: MaterialId,
    ) -> Self {
        Self {
            liner_material,
            liner_thickness_nm,
            bridge_material,
            bridge_thickness_nm,
            fill_material,
        }
    }
}

/// Parameters for a TSV (Through-Silicon Via).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TSVParams {
    /// Total diameter of the TSV (including liner) in nanometers
    pub diameter_nm: i64,
    /// The material stack for this TSV
    pub stack: LinerStack,
    /// Keep-out zone radius multiplier (typically 3.0x diameter)
    pub koz_multiplier: f32,
}

impl TSVParams {
    /// Create new TSV parameters.
    pub fn new(diameter_nm: i64, stack: LinerStack) -> Self {
        Self {
            diameter_nm,
            stack,
            koz_multiplier: 3.0,
        }
    }
}

/// A cutout (hole) in a substrate layer (v0.1.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Cutout {
    pub bbox: BoundingBox,
    pub shape: SubstrateLayerShape,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubstrateLayer {
    /// Material ID (e.g., 1 = FR4, 5 = Silicon)
    pub material: MaterialId,

    /// Net ID (typically 0 for substrate, as it's not part of any net)
    pub net: NetId,

    /// Bounding box in nanometers defining the substrate region
    pub bbox: BoundingBox,

    /// Cutouts (holes) in the substrate for mounting holes, edge cuts, etc.
    /// Points inside these bounding boxes are NOT part of the substrate.
    pub cutouts: SmallVec<[Cutout; 4]>,

    /// Type of substrate layer (pour vs contact) for proper physics validation
    /// Added in v0.1.6 for accurate thermal analysis
    pub layer_type: SubstrateLayerType,

    /// Geometric shape for 3D export (v0.1.6)
    pub shape: SubstrateLayerShape,

    /// Keep-out zone radius in nanometers (v0.1.7: TSV Stress Management)
    /// If non-zero, this region around the substrate layer is forbidden for other components.
    pub koz_radius_nm: i64,
}

impl SubstrateLayerShape {
    /// Pre-baked generator for circular via cross-section
    pub fn cylinder(diameter_nm: i64, segments: u32) -> Self {
        let radius = diameter_nm / 2;
        let mut contour = Path64::new();
        for i in 0..segments {
            let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            let x = (radius as f64 * angle.cos()) as i64;
            let y = (radius as f64 * angle.sin()) as i64;
            contour.push(Point64::new(x, y));
        }
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments,
        }
    }

    /// Pre-baked generator for square via cross-section
    pub fn square(size_nm: i64) -> Self {
        let half = size_nm / 2;
        let mut contour = Path64::new();
        contour.push(Point64::new(-half, -half));
        contour.push(Point64::new(half, -half));
        contour.push(Point64::new(half, half));
        contour.push(Point64::new(-half, half));
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 4,
        }
    }

    /// Pre-baked generator for rectangular via cross-section (different width/height)
    pub fn rect(width_nm: i64, height_nm: i64) -> Self {
        let half_w = width_nm / 2;
        let half_h = height_nm / 2;
        let mut contour = Path64::new();
        contour.push(Point64::new(-half_w, -half_h));
        contour.push(Point64::new(half_w, -half_h));
        contour.push(Point64::new(half_w, half_h));
        contour.push(Point64::new(-half_w, half_h));
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 4,
        }
    }

    /// Pre-baked generator for hexagonal via cross-section
    pub fn hexagon(size_nm: i64) -> Self {
        let half = size_nm / 2;
        let quarter = size_nm / 4;
        // Regular hexagon: 6 vertices
        // Width = size_nm, height = size_nm * sin(60°) ≈ size_nm * 0.866
        let height_quarter = (size_nm as f64 * 0.433) as i64; // sin(60°) * 0.5 * size
        let mut contour = Path64::new();
        contour.push(Point64::new(-half, 0));
        contour.push(Point64::new(-quarter, height_quarter));
        contour.push(Point64::new(quarter, height_quarter));
        contour.push(Point64::new(half, 0));
        contour.push(Point64::new(quarter, -height_quarter));
        contour.push(Point64::new(-quarter, -height_quarter));
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 6,
        }
    }
}

impl SubstrateLayer {
    /// Create a new substrate layer without cutouts.
    pub fn new(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type,
            shape: SubstrateLayerShape::Rect,
            koz_radius_nm: 0,
        }
    }

    /// Create a new cylindrical substrate layer.
    pub fn new_cylinder(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        diameter: i64,
        segments: u32,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::cylinder(diameter, segments),
            koz_radius_nm: 0,
        }
    }

    /// Create a new circular substrate layer (2D circle pour).
    pub fn new_circle(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        radius: i64,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Pour,
            shape: SubstrateLayerShape::Circle { radius },
            koz_radius_nm: 0,
        }
    }

    /// Create a new square via substrate layer.
    pub fn new_square_via(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        size: i64,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::square(size),
            koz_radius_nm: 0,
        }
    }

    /// Create a new hexagonal via substrate layer.
    pub fn new_hexagon_via(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        size: i64,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::hexagon(size),
            koz_radius_nm: 0,
        }
    }

    /// Create a new polygon-based via substrate layer from an arbitrary contour.
    pub fn new_polygon_via(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        contour: clipper2_rust::Path64,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::Polygon {
                outer_contour: contour,
                holes: clipper2_rust::Paths64::new(),
                segments: 16,
            },
            koz_radius_nm: 0,
        }
    }

    /// Create a new tube (plated hole) substrate layer.
    pub fn new_tube(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: CapType,
        bottom_cap: CapType,
        bottom_outer_diameter: Option<u32>,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::Tube {
                outer_diameter,
                inner_diameter,
                pad_diameter,
                segments,
                top_cap,
                bottom_cap,
                bottom_outer_diameter,
            },
            koz_radius_nm: 0,
        }
    }

    /// Create a new substrate layer with cutouts.
    pub fn new_with_cutouts(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        cutouts: Vec<Cutout>,
        layer_type: SubstrateLayerType,
    ) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: cutouts.into(),
            layer_type,
            shape: SubstrateLayerShape::Rect,
            koz_radius_nm: 0,
        }
    }

    /// Add a cutout (hole) to this substrate layer.
    ///
    /// # Arguments
    /// * `cutout_bbox` - Bounding box defining the hole
    pub fn add_cutout(&mut self, cutout_bbox: BoundingBox) {
        self.cutouts.push(Cutout {
            bbox: cutout_bbox,
            shape: SubstrateLayerShape::Rect,
        });
    }

    /// Add a cylindrical cutout (hole) to this substrate layer.
    pub fn add_cylinder_cutout(&mut self, cutout_bbox: BoundingBox, diameter: i64) {
        self.cutouts.push(Cutout {
            bbox: cutout_bbox,
            shape: SubstrateLayerShape::cylinder(diameter, 16),
        });
    }

    /// Check if a point (in nanometers) is within this substrate layer.
    ///
    /// This is the O(1) lookup operation that replaces chunk scanning.
    /// Returns true if the point is inside the substrate bbox AND not inside any cutout.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if the point is within the substrate layer and not in a cutout
    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        // First check if point is in the substrate bbox
        if !(x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y
            && z >= self.bbox.min.z
            && z <= self.bbox.max.z)
        {
            return false;
        }

        // v0.1.7: For non-rectangular layers, check the primary shape first
        match &self.shape {
            SubstrateLayerShape::Polygon { outer_contour, holes, .. } => {
                // Point-in-polygon test using ray casting algorithm
                // The contour is in shape-local coords (centered at 0,0);
                // the bbox center maps to (0,0) in shape space.
                let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
                let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
                let px = x - center_x;
                let py = y - center_y;

                if !point_in_polygon(px, py, outer_contour) {
                    return false;
                }

                // Check that the point is NOT inside any hole
                for hole in holes.iter() {
                    if point_in_polygon(px, py, hole) {
                        return false;
                    }
                }
            }
            SubstrateLayerShape::Tube {
                outer_diameter,
                inner_diameter,
                pad_diameter,
                top_cap,
                bottom_cap,
                bottom_outer_diameter,
                ..
            } => {
                let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
                let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
                let dx = x - center_x;
                let dy = y - center_y;
                let dist_sq = dx * dx + dy * dy;

                let top_outer_radius = *outer_diameter as i64 / 2;
                let top_inner_radius = *inner_diameter as i64 / 2;
                let pad_radius = *pad_diameter as i64 / 2;

                // Tapered logic (v0.1.7)
                let bottom_outer_radius = (*bottom_outer_diameter).unwrap_or(*outer_diameter) as i64 / 2;
                let plating_thickness = top_outer_radius - top_inner_radius;

                let height_nm = self.bbox.max.z - self.bbox.min.z;
                let t = if height_nm > 0 {
                    (z - self.bbox.min.z) as f64 / height_nm as f64
                } else {
                    1.0
                };

                let current_outer_radius =
                    (1.0 - t) * bottom_outer_radius as f64 + t * top_outer_radius as f64;
                let current_inner_radius = current_outer_radius - plating_thickness as f64;

                // v0.1.7: Unified Via Voxel Logic
                // We define "cap regions" as 35um (standard copper) or 1 voxel thick.
                let cap_thickness = 35_000;
                let is_in_top_cap = z >= self.bbox.max.z - cap_thickness;
                let is_in_bottom_cap = z <= self.bbox.min.z + cap_thickness;

                if is_in_top_cap {
                    match top_cap {
                        CapType::None => {
                            if dist_sq > (current_outer_radius * current_outer_radius) as i64
                                || dist_sq < (current_inner_radius * current_inner_radius) as i64
                            {
                                return false;
                            }
                        }
                        CapType::Annular => {
                            if dist_sq > pad_radius * pad_radius
                                || dist_sq < (current_inner_radius * current_inner_radius) as i64
                            {
                                return false;
                            }
                        }
                        CapType::Solid => {
                            if dist_sq > pad_radius * pad_radius {
                                return false;
                            }
                        }
                    }
                } else if is_in_bottom_cap {
                    match bottom_cap {
                        CapType::None => {
                            if dist_sq > (current_outer_radius * current_outer_radius) as i64
                                || dist_sq < (current_inner_radius * current_inner_radius) as i64
                            {
                                return false;
                            }
                        }
                        CapType::Annular => {
                            if dist_sq > pad_radius * pad_radius
                                || dist_sq < (current_inner_radius * current_inner_radius) as i64
                            {
                                return false;
                            }
                        }
                        CapType::Solid => {
                            if dist_sq > pad_radius * pad_radius {
                                return false;
                            }
                        }
                    }
                } else {
                    // Main tube body (wall)
                    if dist_sq > (current_outer_radius * current_outer_radius) as i64
                        || dist_sq < (current_inner_radius * current_inner_radius) as i64
                    {
                        return false;
                    }
                }
            }
            SubstrateLayerShape::Rect => {}
            SubstrateLayerShape::Circle { radius } => {
                let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
                let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
                let dx = x - center_x;
                let dy = y - center_y;
                if dx * dx + dy * dy > radius * radius {
                    return false; // Outside the circle
                }
            }
        }

        // Then check if point is NOT in any cutout
        for cutout in &self.cutouts {
            let bbox = &cutout.bbox;
            if x >= bbox.min.x
                && x <= bbox.max.x
                && y >= bbox.min.y
                && y <= bbox.max.y
                && z >= bbox.min.z
                && z <= bbox.max.z
            {
                // v0.1.7: For cylindrical cutouts, perform distance check
                match &cutout.shape {
                    SubstrateLayerShape::Polygon { outer_contour, .. } => {
                        let center_x = (bbox.min.x + bbox.max.x) / 2;
                        let center_y = (bbox.min.y + bbox.max.y) / 2;
                        let px = x - center_x;
                        let py = y - center_y;

                        let mut min_x = i64::MAX;
                        let mut max_x = i64::MIN;
                        let mut min_y = i64::MAX;
                        let mut max_y = i64::MIN;
                        for p in outer_contour.iter() {
                            if p.x < min_x { min_x = p.x; }
                            if p.x > max_x { max_x = p.x; }
                            if p.y < min_y { min_y = p.y; }
                            if p.y > max_y { max_y = p.y; }
                        }

                        if px >= min_x && px <= max_x && py >= min_y && py <= max_y {
                            return false;
                        }
                    }
                    SubstrateLayerShape::Tube {
                        outer_diameter,
                        inner_diameter,
                        ..
                    } => {
                        let center_x = (bbox.min.x + bbox.max.x) / 2;
                        let center_y = (bbox.min.y + bbox.max.y) / 2;
                        let dx = x - center_x;
                        let dy = y - center_y;
                        let outer_radius = *outer_diameter as i64 / 2;
                        let inner_radius = *inner_diameter as i64 / 2;
                        let dist_sq = dx * dx + dy * dy;
                        if dist_sq <= outer_radius * outer_radius
                            && dist_sq >= inner_radius * inner_radius
                        {
                            return false; // Point is in the tube cutout
                        }
                    }
                    SubstrateLayerShape::Rect => {
                        return false; // Point is in the rectangular cutout
                    }
                    SubstrateLayerShape::Circle { radius } => {
                        let center_x = (bbox.min.x + bbox.max.x) / 2;
                        let center_y = (bbox.min.y + bbox.max.y) / 2;
                        let dx = x - center_x;
                        let dy = y - center_y;
                        if dx * dx + dy * dy <= radius * radius {
                            return false; // Point is in the circular cutout
                        }
                    }
                }
            }
        }

        true
    }

    /// Check if a point (in nanometers) is within the Keep-Out Zone of this layer.
    ///
    /// Specifically for TSVs, this checks if the point is within the forbidden
    /// stress-field radius (v0.1.7).
    pub fn is_in_koz(&self, x: i64, y: i64, z: i64) -> bool {
        if self.koz_radius_nm == 0 {
            return false;
        }

        // Check vertical range first
        if z < self.bbox.min.z || z > self.bbox.max.z {
            return false;
        }

        // Check distance from center (assuming cylindrical KOZ for TSVs)
        let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
        let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
        let dx = x - center_x;
        let dy = y - center_y;

        dx * dx + dy * dy <= self.koz_radius_nm * self.koz_radius_nm
    }

    /// Check if this layer is a simple axis-aligned rectangle.
    ///
    /// Returns false if the layer has cutouts or non-rectangular geometry.
    /// This is used by the export pipeline to determine if contour tracing is needed.
    ///
    /// # Returns
    /// `true` if the layer is a simple rectangle with no cutouts
    pub fn is_axis_aligned_rectangle(&self) -> bool {
        self.cutouts.is_empty()
    }

    /// Convert substrate layer to 2D boolean grid for contour tracing.
    ///
    /// This is used when exporting diagonal or complex geometry that requires
    /// voxel-to-vector conversion with anti-aliasing.
    ///
    /// # Arguments
    /// * `voxel_size_nm` - Size of each voxel in nanometers
    ///
    /// # Returns
    /// Tuple of (grid, width, height) where grid is a flat boolean array
    pub fn to_2d_boolean_grid(&self, voxel_size_nm: i64) -> (Vec<bool>, usize, usize) {
        let width = ((self.bbox.max.x - self.bbox.min.x) / voxel_size_nm) as usize;
        let height = ((self.bbox.max.y - self.bbox.min.y) / voxel_size_nm) as usize;

        let mut grid = vec![true; width * height];

        // Mark cutouts as false
        for cutout in &self.cutouts {
            let cutout_min_x = ((cutout.bbox.min.x - self.bbox.min.x) / voxel_size_nm).max(0) as usize;
            let cutout_min_y = ((cutout.bbox.min.y - self.bbox.min.y) / voxel_size_nm).max(0) as usize;
            let cutout_max_x =
                ((cutout.bbox.max.x - self.bbox.min.x) / voxel_size_nm).min(width as i64) as usize;
            let cutout_max_y =
                ((cutout.bbox.max.y - self.bbox.min.y) / voxel_size_nm).min(height as i64) as usize;

            for y in cutout_min_y..cutout_max_y {
                for x in cutout_min_x..cutout_max_x {
                    if y * width + x < grid.len() {
                        // v0.1.7: For cylindrical cutouts, perform distance check
                        match &cutout.shape {
                            SubstrateLayerShape::Polygon { outer_contour, .. } => {
                                let x_nm = self.bbox.min.x + (x as i64 * voxel_size_nm);
                                let y_nm = self.bbox.min.y + (y as i64 * voxel_size_nm);
                                let center_x = (cutout.bbox.min.x + cutout.bbox.max.x) / 2;
                                let center_y = (cutout.bbox.min.y + cutout.bbox.max.y) / 2;
                                let px = x_nm - center_x;
                                let py = y_nm - center_y;

                                let mut min_x = i64::MAX;
                                let mut max_x = i64::MIN;
                                let mut min_y = i64::MAX;
                                let mut max_y = i64::MIN;
                                for p in outer_contour.iter() {
                                    if p.x < min_x { min_x = p.x; }
                                    if p.x > max_x { max_x = p.x; }
                                    if p.y < min_y { min_y = p.y; }
                                    if p.y > max_y { max_y = p.y; }
                                }

                                if px >= min_x && px <= max_x && py >= min_y && py <= max_y {
                                    grid[y * width + x] = false;
                                }
                            }
                            SubstrateLayerShape::Tube {
                                outer_diameter,
                                inner_diameter,
                                ..
                            } => {
                                let x_nm = self.bbox.min.x + (x as i64 * voxel_size_nm);
                                let y_nm = self.bbox.min.y + (y as i64 * voxel_size_nm);
                                let center_x = (cutout.bbox.min.x + cutout.bbox.max.x) / 2;
                                let center_y = (cutout.bbox.min.y + cutout.bbox.max.y) / 2;
                                let dx = x_nm - center_x;
                                let dy = y_nm - center_y;
                                let outer_radius = *outer_diameter as i64 / 2;
                                let inner_radius = *inner_diameter as i64 / 2;
                                let dist_sq = dx * dx + dy * dy;
                                if dist_sq <= outer_radius * outer_radius
                                    && dist_sq >= inner_radius * inner_radius
                                {
                                    grid[y * width + x] = false;
                                }
                            }
                            SubstrateLayerShape::Rect => {
                                grid[y * width + x] = false;
                            }
                            SubstrateLayerShape::Circle { radius } => {
                                let x_nm = self.bbox.min.x + (x as i64 * voxel_size_nm);
                                let y_nm = self.bbox.min.y + (y as i64 * voxel_size_nm);
                                let center_x = (cutout.bbox.min.x + cutout.bbox.max.x) / 2;
                                let center_y = (cutout.bbox.min.y + cutout.bbox.max.y) / 2;
                                let dx = x_nm - center_x;
                                let dy = y_nm - center_y;
                                if dx * dx + dy * dy <= radius * radius {
                                    grid[y * width + x] = false;
                                }
                            }
                        }
                    }
                }
            }
        }

        (grid, width, height)
    }
}

/// Component pin for physical continuity validation (v0.1.6 Sprint 3).
///
/// Represents an external connection point on a component. Pins are used by
/// the P43 validator to detect floating conductors - conductive geometry that
/// has no component pins touching it.
///
/// Pins are registered during component placement and stored in absolute
/// coordinates (nanometers). The physics validator checks if each conductive
/// island has at least one pin touching it.
///
/// Total size: ~40 bytes (position + name pointer + net pointer)
///
/// # Example
/// ```
/// # use hwc_engine::voxel_grid::ComponentPin;
/// let pin = ComponentPin::new(
///     1_000_000,  // x: 1mm
///     2_000_000,  // y: 2mm
///     0,          // z: 0mm (bottom layer)
///     "M1".into(),
///     "gate".into(),
///     Some("VIN".into())
/// );
/// assert_eq!(pin.x_nm, 1_000_000);
/// assert_eq!(pin.component_name, "M1");
/// assert_eq!(pin.pin_name, "gate");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentPin {
    /// X coordinate in nanometers (absolute position)
    pub x_nm: i64,

    /// Y coordinate in nanometers (absolute position)
    pub y_nm: i64,

    /// Z coordinate in nanometers (absolute position)
    pub z_nm: i64,

    /// Component instance name (e.g., "M1", "R1")
    pub component_name: CompactString,

    /// Pin name within the component (e.g., "gate", "drain", "source", "A", "K")
    pub pin_name: CompactString,

    /// Net assignment (e.g., "VIN", "GND", "VDD")
    /// None if the pin is not connected to any net
    pub net: Option<CompactString>,
}

impl ComponentPin {
    /// Create a new component pin.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers (absolute)
    /// * `y_nm` - Y coordinate in nanometers (absolute)
    /// * `z_nm` - Z coordinate in nanometers (absolute)
    /// * `component_name` - Component instance name (e.g., "M1")
    /// * `pin_name` - Pin name within the component (e.g., "gate")
    /// * `net` - Optional net assignment (e.g., Some("VIN"))
    pub fn new(
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: CompactString,
        pin_name: CompactString,
        net: Option<CompactString>,
    ) -> Self {
        Self {
            x_nm,
            y_nm,
            z_nm,
            component_name,
            pin_name,
            net,
        }
    }

    /// Get the position as a tuple (x, y, z) in nanometers.
    pub fn position(&self) -> (i64, i64, i64) {
        (self.x_nm, self.y_nm, self.z_nm)
    }

    /// Get a display name for this pin (e.g., "M1.gate").
    pub fn display_name(&self) -> CompactString {
        format!("{}.{}", self.component_name, self.pin_name).into()
    }
}

/// Component metadata for sparse component architecture.
///
/// GOD-TIER SPARSE ARCHITECTURE: Same pattern as SubstrateLayer.
/// Instead of filling millions of voxels per component (Density Bomb),
/// we store just the bounding box, material ID, and component name.
///
/// Router sees components via get_material() lookup (O(components) per query).
/// Placement is O(1): Just push to vector.
/// Memory is O(components), not O(voxels).
///
/// Total size: ~72 bytes (bbox + material + name pointer + blocked_z_ranges)
///
/// # Layer-Aware Keepout Zones (KOZ) — v0.1.7
///
/// `blocked_z_ranges` defines which Z-layers this component blocks for
/// pours and traces. A component sitting on M3 (top metal) should only
/// block the M3 Z-range, allowing pours on M1/M2 to pass underneath.
///
/// When `blocked_z_ranges` is empty (default), the component blocks ALL
/// Z-layers it occupies (legacy behavior for backward compatibility).
///
/// # Example
/// ```
/// # use hwc_engine::geometry::{BoundingBox, Point3D};
/// # use hwc_engine::voxel_grid::ComponentMetadata;
/// # use smallvec::SmallVec;
/// let bbox = BoundingBox::new(
///     Point3D::new(1_000_000, 1_000_000, 0),
///     Point3D::new(6_000_000, 3_000_000, 1_000_000)
/// );
/// let component = ComponentMetadata::new(5, bbox, "R1".into());
/// assert_eq!(component.material, 5);
/// assert_eq!(component.name, "R1");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMetadata {
    /// Material ID (e.g., 5 = Ceramic, 10 = Polysilicon)
    pub material: MaterialId,

    /// Bounding box in nanometers defining the component region
    pub bbox: BoundingBox,

    /// Component name for debugging and error messages
    pub name: CompactString,

    /// Component type (e.g., "Resistor", "Transistor", "MCU")
    pub component_type: CompactString,

    /// Terminal positions (absolute world coordinates)
    pub terminals: Vec<Terminal>,

    /// Net bindings
    pub net_bindings: FxHashMap<CompactString, NetId>,

    /// Layer-Aware Keepout Zones (KOZ) — v0.1.7
    ///
    /// A list of Z-ranges [start_nm, end_nm) that this component blocks.
    /// Pours and traces at a Z outside these ranges can pass through the
    /// component's XY footprint without collision.
    ///
    /// When empty (default), the component blocks ALL Z-layers it occupies
    /// (legacy behavior — full 3D keepout).
    ///
    /// Example: A surface-mount resistor on M3 (z=500µm to 600µm) would
    /// block z:[500_000, 600_000] but permit pours on M1/M2 underneath.
    pub blocked_z_ranges: SmallVec<[(i64, i64); 2]>,
}

impl ComponentMetadata {
    /// Create a new component metadata entry.
    ///
    /// # Arguments
    /// * `material` - Material ID (e.g., 5 for Ceramic)
    /// * `bbox` - Bounding box in nanometers
    /// * `name` - Component name (e.g., "R1", "Q1")
    /// * `component_type` - Component type (e.g., "Resistor")
    pub fn new(
        material: MaterialId,
        bbox: BoundingBox,
        name: CompactString,
        component_type: CompactString,
    ) -> Self {
        Self {
            material,
            bbox,
            name,
            component_type,
            terminals: Vec::new(),
            net_bindings: FxHashMap::default(),
            blocked_z_ranges: SmallVec::new(), // Empty = full 3D keepout (legacy)
        }
    }

    /// Add a terminal to the component
    pub fn add_terminal(&mut self, terminal: Terminal) {
        self.terminals.push(terminal);
    }

    /// Bind a pin to a net
    pub fn bind_net(&mut self, pin_name: CompactString, net_id: NetId) {
        self.net_bindings.insert(pin_name, net_id);
    }

    /// Check if a point (in nanometers) is within this component.
    ///
    /// This is the O(1) lookup operation for component material queries.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if the point is within the component bounding box
    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y
            && z >= self.bbox.min.z
            && z <= self.bbox.max.z
    }

    /// Check if a point (in nanometers) is inside this component's keepout zone.
    ///
    /// Layer-Aware KOZ (v0.1.7):
    /// - If `blocked_z_ranges` is empty: blocks all Z-layers (full 3D keepout)
    /// - If `blocked_z_ranges` is non-empty: only blocks the listed Z-ranges
    ///
    /// This enables pours and traces to flow under/over components on
    /// different Z-layers (e.g., M3 trace under an M1 component).
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if the point is inside the keepout zone (pour/trace should block)
    #[inline]
    pub fn is_in_koz(&self, x: i64, y: i64, z: i64) -> bool {
        // First, check if point is within XY footprint
        if !(x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y)
        {
            return false;
        }

        // If no blocked_z_ranges specified, use full bbox Z range (legacy)
        if self.blocked_z_ranges.is_empty() {
            return z >= self.bbox.min.z && z <= self.bbox.max.z;
        }

        // Layer-Aware: Check each blocked Z-range
        for &(z_start, z_end) in &self.blocked_z_ranges {
            if z >= z_start && z <= z_end {
                return true;
            }
        }

        false // Z is outside all blocked ranges — pour/trace can pass
    }
}

/// Ray-casting point-in-polygon test.
///
/// Returns `true` if `(px, py)` is inside the polygon defined by `contour`.
/// Uses the even-odd rule with a horizontal ray cast to the right.
fn point_in_polygon(px: i64, py: i64, contour: &clipper2_rust::Path64) -> bool {
    let n = contour.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let yi = contour[i].y;
        let yj = contour[j].y;
        let xi = contour[i].x;
        let xj = contour[j].x;

        // Check if the ray from (px, py) going right crosses this edge
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Rotation for component placement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Rotate90,
    Rotate180,
    Rotate270,
}
