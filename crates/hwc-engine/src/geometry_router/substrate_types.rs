//! Substrate and component types — gridless, vector-first representations.
//!
//! These types define the physical substrate layers, component metadata,
//! and pin positions used throughout the engine. They are independent of
//! any grid and represent pure continuous geometry.

use crate::geometry::{BoundingBox, Point3D};
use clipper2_rust::{Path64, Paths64};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

// Re-export MaterialId from the material module
pub use crate::material::MaterialId;
pub use hwc_physics::connectivity::SubstrateLayerType;

/// Net ID type — u32 for up to 4 billion nets.
pub type NetId = u32;

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

/// Type of cap for tube shapes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    /// No cap (open end)
    None,
    /// Annular ring (disk with a hole)
    Annular,
    /// Solid disk (no hole)
    Solid,
}

/// Physical shape of the substrate layer
#[derive(Debug, Clone, PartialEq)]
pub enum SubstrateLayerShape {
    /// Axis-aligned bounding box (default)
    Rect,
    /// 2D circle shape (circular pours, annular rings)
    Circle {
        /// Radius in nanometers
        radius: i64,
    },
    /// Generic polygon-based shape.
    /// The outer_contour defines the boundary; holes are subtracted from it.
    Polygon {
        outer_contour: Path64,
        holes: Paths64,
        /// Tessellation segments for 3D rendering
        segments: u32,
    },
    /// Tube shape (Plated through-hole walls)
    Tube {
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: CapType,
        bottom_cap: CapType,
        /// Bottom outer diameter for tapered vias
        bottom_outer_diameter: Option<u32>,
    },
}

/// A multi-material stack for TSVs (Through-Silicon Vias).
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

/// A cutout (hole) in a substrate layer.
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

    /// Pre-baked generator for rectangular via cross-section
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

/// A substrate layer represented as a bounding box with uniform material.
///
/// This is the core of the sparse substrate architecture. Instead of allocating
/// millions of chunks for a uniform substrate layer, we store just the bounding
/// box and material ID.
#[derive(Debug, Clone, PartialEq)]
pub struct SubstrateLayer {
    /// Material ID (e.g., 1 = FR4, 5 = Silicon)
    pub material: MaterialId,
    /// Net ID (typically 0 for substrate)
    pub net: NetId,
    /// Bounding box in nanometers defining the substrate region
    pub bbox: BoundingBox,
    /// Cutouts (holes) in the substrate
    pub cutouts: SmallVec<[Cutout; 4]>,
    /// Type of substrate layer
    pub layer_type: SubstrateLayerType,
    /// Geometric shape for 3D export
    pub shape: SubstrateLayerShape,
    /// Keep-out zone radius in nanometers
    pub koz_radius_nm: i64,
    /// Child regions for merged trace segments
    pub regions: SmallVec<[BoundingBox; 4]>,
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
            regions: SmallVec::new(),
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
            regions: SmallVec::new(),
        }
    }

    /// Create a new circular substrate layer (2D circle pour).
    pub fn new_circle(material: MaterialId, net: NetId, bbox: BoundingBox, radius: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Pour,
            shape: SubstrateLayerShape::Circle { radius },
            koz_radius_nm: 0,
            regions: SmallVec::new(),
        }
    }

    /// Create a new square via substrate layer.
    pub fn new_square_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::square(size),
            koz_radius_nm: 0,
            regions: SmallVec::new(),
        }
    }

    /// Create a new hexagonal via substrate layer.
    pub fn new_hexagon_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::hexagon(size),
            koz_radius_nm: 0,
            regions: SmallVec::new(),
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
            regions: SmallVec::new(),
        }
    }

    /// Create a new tube (plated hole) substrate layer.
    #[allow(clippy::too_many_arguments)]
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
            regions: SmallVec::new(),
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
            regions: SmallVec::new(),
        }
    }

    /// Append a child region (bounding box) to this layer.
    pub fn append_region(&mut self, bbox: BoundingBox) {
        self.regions.push(bbox);
    }

    /// Add a cutout (hole) to this substrate layer.
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
    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        if !self.regions.is_empty() {
            let in_any_region = self.regions.iter().any(|r| {
                x >= r.min.x
                    && x <= r.max.x
                    && y >= r.min.y
                    && y <= r.max.y
                    && z >= r.min.z
                    && z <= r.max.z
            });
            if !in_any_region {
                return false;
            }
        } else {
            if !(x >= self.bbox.min.x
                && x <= self.bbox.max.x
                && y >= self.bbox.min.y
                && y <= self.bbox.max.y
                && z >= self.bbox.min.z
                && z <= self.bbox.max.z)
            {
                return false;
            }
        }

        match &self.shape {
            SubstrateLayerShape::Polygon {
                outer_contour,
                holes,
                ..
            } => {
                let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
                let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
                let px = x - center_x;
                let py = y - center_y;

                if !point_in_polygon(px, py, outer_contour) {
                    return false;
                }

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

                let bottom_outer_radius =
                    (*bottom_outer_diameter).unwrap_or(*outer_diameter) as i64 / 2;
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
                } else if dist_sq > (current_outer_radius * current_outer_radius) as i64
                    || dist_sq < (current_inner_radius * current_inner_radius) as i64
                {
                    return false;
                }
            }
            SubstrateLayerShape::Rect => {}
            SubstrateLayerShape::Circle { radius } => {
                let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
                let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
                let dx = x - center_x;
                let dy = y - center_y;
                if dx * dx + dy * dy > radius * radius {
                    return false;
                }
            }
        }

        for cutout in &self.cutouts {
            let bbox = &cutout.bbox;
            if x >= bbox.min.x
                && x <= bbox.max.x
                && y >= bbox.min.y
                && y <= bbox.max.y
                && z >= bbox.min.z
                && z <= bbox.max.z
            {
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
                            if p.x < min_x {
                                min_x = p.x;
                            }
                            if p.x > max_x {
                                max_x = p.x;
                            }
                            if p.y < min_y {
                                min_y = p.y;
                            }
                            if p.y > max_y {
                                max_y = p.y;
                            }
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
                            return false;
                        }
                    }
                    SubstrateLayerShape::Rect => {
                        return false;
                    }
                    SubstrateLayerShape::Circle { radius } => {
                        let center_x = (bbox.min.x + bbox.max.x) / 2;
                        let center_y = (bbox.min.y + bbox.max.y) / 2;
                        let dx = x - center_x;
                        let dy = y - center_y;
                        if dx * dx + dy * dy <= radius * radius {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }

    /// Check if a point (in nanometers) is within the Keep-Out Zone of this layer.
    pub fn is_in_koz(&self, x: i64, y: i64, z: i64) -> bool {
        if self.koz_radius_nm == 0 {
            return false;
        }
        if z < self.bbox.min.z || z > self.bbox.max.z {
            return false;
        }
        let center_x = (self.bbox.min.x + self.bbox.max.x) / 2;
        let center_y = (self.bbox.min.y + self.bbox.max.y) / 2;
        let dx = x - center_x;
        let dy = y - center_y;
        dx * dx + dy * dy <= self.koz_radius_nm * self.koz_radius_nm
    }

    /// Check if this layer is a simple axis-aligned rectangle.
    pub fn is_axis_aligned_rectangle(&self) -> bool {
        self.cutouts.is_empty()
    }
}

/// Ray-casting point-in-polygon test.
pub(super) fn point_in_polygon(px: i64, py: i64, contour: &Path64) -> bool {
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

        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Component pin for physical continuity validation.
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
    pub net: Option<CompactString>,
}

impl ComponentPin {
    /// Create a new component pin.
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
    /// Layer-Aware Keepout Zones (KOZ)
    pub blocked_z_ranges: SmallVec<[(i64, i64); 2]>,
}

impl ComponentMetadata {
    /// Create a new component metadata entry.
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
            blocked_z_ranges: SmallVec::new(),
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
    #[inline]
    pub fn is_in_koz(&self, x: i64, y: i64, z: i64) -> bool {
        if !(x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y)
        {
            return false;
        }

        if self.blocked_z_ranges.is_empty() {
            return z >= self.bbox.min.z && z <= self.bbox.max.z;
        }

        for &(z_start, z_end) in &self.blocked_z_ranges {
            if z >= z_start && z <= z_end {
                return true;
            }
        }

        false
    }

    /// v0.1.8: Check if this component has physical material on a given Z-layer.
    ///
    /// A component "has material" on a layer if its Z-range (from `bbox.min.z`
    /// to `bbox.max.z`) overlaps the layer's Z-range. This is used for
    /// layer-aware interior lockout — the router only blocks cost on layers
    /// where the component actually has physical material.
    ///
    /// # Arguments
    /// * `layer_z_min` - Lower Z bound of the layer (inclusive)
    /// * `layer_z_max` - Upper Z bound of the layer (exclusive)
    #[inline]
    pub fn has_material_on_z_range(&self, layer_z_min: i64, layer_z_max: i64) -> bool {
        // Overlap test: component Z-range intersects layer Z-range
        self.bbox.min.z < layer_z_max && self.bbox.max.z > layer_z_min
    }

    /// v0.1.8: Compute the boundary port for a pin on this component.
    ///
    /// The boundary port is the intersection of the pin's XY position with
    /// the component's bounding box surface. Traces must terminate at
    /// boundary ports — never at the pin's interior coordinates.
    ///
    /// # Arguments
    /// * `pin_x` - X coordinate of the pin (nm)
    /// * `pin_y` - Y coordinate of the pin (nm)
    /// * `pin_z` - Z coordinate of the pin (nm)
    /// * `direction` - Cardinal direction toward which the trace exits
    ///
    /// # Returns
    /// The boundary port (x, y, z) on the component's bounding box face.
    pub fn boundary_port(
        &self,
        pin_x: i64,
        pin_y: i64,
        pin_z: i64,
        direction: CardinalDirection,
    ) -> (i64, i64, i64) {
        match direction {
            CardinalDirection::North => (pin_x, self.bbox.max.y, pin_z),
            CardinalDirection::South => (pin_x, self.bbox.min.y, pin_z),
            CardinalDirection::East => (self.bbox.max.x, pin_y, pin_z),
            CardinalDirection::West => (self.bbox.min.x, pin_y, pin_z),
        }
    }
}

/// Cardinal directions for boundary port computation (v0.1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

/// Compaction statistics for monitoring memory health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionStats {
    /// Total number of slots in the page directory
    pub total_slots: usize,
    /// Number of allocated slots
    pub allocated_chunks: usize,
    /// Number of empty slots (zombie)
    pub zombie_chunks: usize,
    /// Number of active slots (occupied)
    pub active_chunks: usize,
    /// Zombie ratio (zombie / allocated)
    pub zombie_ratio: f64,
}
