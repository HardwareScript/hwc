use super::super::chunk::{MaterialId, NetId};
use crate::geometry::{BoundingBox, Point3D};
use clipper2_rust::{Path64, Paths64};
use compact_str::CompactString;

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
        pad_diameter: u32, // v0.1.7: Unified Via support
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

/// Rotation for component placement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Rotate90,
    Rotate180,
    Rotate270,
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
            contour.push(clipper2_rust::Point64::new(x, y));
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
        let contour = vec![
            clipper2_rust::Point64::new(-half, -half),
            clipper2_rust::Point64::new(half, -half),
            clipper2_rust::Point64::new(half, half),
            clipper2_rust::Point64::new(-half, half),
        ];
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
        let contour = vec![
            clipper2_rust::Point64::new(-half_w, -half_h),
            clipper2_rust::Point64::new(half_w, -half_h),
            clipper2_rust::Point64::new(half_w, half_h),
            clipper2_rust::Point64::new(-half_w, half_h),
        ];
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
        let height_quarter = (size_nm as f64 * 0.433) as i64;
        let contour = vec![
            clipper2_rust::Point64::new(-half, 0),
            clipper2_rust::Point64::new(-quarter, height_quarter),
            clipper2_rust::Point64::new(quarter, height_quarter),
            clipper2_rust::Point64::new(half, 0),
            clipper2_rust::Point64::new(quarter, -height_quarter),
            clipper2_rust::Point64::new(-quarter, -height_quarter),
        ];
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 6,
        }
    }
}
