use super::super::chunk::{MaterialId, NetId};
use super::types::{CapType, Cutout, SubstrateLayerShape, SubstrateLayerType};
use crate::geometry::BoundingBox;
use clipper2_rust::Path64;
use smallvec::SmallVec;

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
    pub fn new_circle(material: MaterialId, net: NetId, bbox: BoundingBox, radius: i64) -> Self {
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
    pub fn new_square_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
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
    pub fn new_hexagon_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
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
        if !(x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y
            && z >= self.bbox.min.z
            && z <= self.bbox.max.z)
        {
            return false;
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
    ///
    /// Specifically for TSVs, this checks if the point is within the forbidden
    /// stress-field radius (v0.1.7).
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

        for cutout in &self.cutouts {
            let cutout_min_x =
                ((cutout.bbox.min.x - self.bbox.min.x) / voxel_size_nm).max(0) as usize;
            let cutout_min_y =
                ((cutout.bbox.min.y - self.bbox.min.y) / voxel_size_nm).max(0) as usize;
            let cutout_max_x =
                ((cutout.bbox.max.x - self.bbox.min.x) / voxel_size_nm).min(width as i64) as usize;
            let cutout_max_y =
                ((cutout.bbox.max.y - self.bbox.min.y) / voxel_size_nm).min(height as i64) as usize;

            for y in cutout_min_y..cutout_max_y {
                for x in cutout_min_x..cutout_max_x {
                    if y * width + x < grid.len() {
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

/// Ray-casting point-in-polygon test.
///
/// Returns `true` if `(px, py)` is inside the polygon defined by `contour`.
/// Uses the even-odd rule with a horizontal ray cast to the right.
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
