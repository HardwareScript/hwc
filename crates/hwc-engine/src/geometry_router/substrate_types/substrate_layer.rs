use crate::geometry::BoundingBox;
use hwc_physics::connectivity::SubstrateLayerType;
use smallvec::SmallVec;

use super::core_types::{Cutout, MaterialId, NetId, TubeSpec};
use super::shapes::SubstrateLayerShape;

#[derive(Debug, Clone, PartialEq)]
pub struct SubstrateLayer {
    pub material: MaterialId,
    pub net: NetId,
    pub bbox: BoundingBox,
    pub layer_name: compact_str::CompactString,
    pub layer_id: Option<hwc_types::LayerId>,
    pub cutouts: SmallVec<[Cutout; 4]>,
    pub layer_type: SubstrateLayerType,
    pub shape: SubstrateLayerShape,
    pub koz_radius_nm: i64,
    pub regions: SmallVec<[BoundingBox; 4]>,
    /// Device terminal binding (v0.2.1) - if present, this layer is part of a device terminal
    pub device_binding: Option<(String, String)>, // (device_name, terminal)
}

impl SubstrateLayer {
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
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type,
            shape: SubstrateLayerShape::Rect,
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn with_layer_name(mut self, name: impl Into<compact_str::CompactString>) -> Self {
        self.layer_name = name.into();
        self
    }

    pub fn with_layer_id(mut self, id: hwc_types::LayerId) -> Self {
        self.layer_id = Some(id);
        self
    }

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
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::cylinder(diameter, segments),
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn new_circle(material: MaterialId, net: NetId, bbox: BoundingBox, radius: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Pour,
            shape: SubstrateLayerShape::Circle { radius },
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn new_contact_circle(material: MaterialId, net: NetId, bbox: BoundingBox, radius: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::Circle { radius },
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn new_square_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::square(size),
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn new_hexagon_via(material: MaterialId, net: NetId, bbox: BoundingBox, size: i64) -> Self {
        Self {
            material,
            net,
            bbox,
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::hexagon(size),
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

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
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::new(),
            layer_type: SubstrateLayerType::Contact,
            shape: SubstrateLayerShape::Polygon {
                outer_contour: contour,
                holes: clipper2_rust::Paths64::new(),
                segments: 16,
            },
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn new_tube(spec: TubeSpec) -> Self {
        let TubeSpec {
            material,
            net,
            bbox,
            outer_diameter,
            inner_diameter,
            pad_diameter,
            segments,
            top_cap,
            bottom_cap,
            bottom_outer_diameter,
        } = spec;
        Self {
            material,
            net,
            bbox,
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
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
            device_binding: None,
        }
    }

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
            layer_name: compact_str::CompactString::default(),
            layer_id: None,
            cutouts: SmallVec::from_vec(cutouts),
            layer_type,
            shape: SubstrateLayerShape::Rect,
            koz_radius_nm: 0,
            regions: SmallVec::new(),
            device_binding: None,
        }
    }

    pub fn append_region(&mut self, bbox: BoundingBox) {
        self.regions.push(bbox);
    }

    pub fn add_cutout(&mut self, cutout_bbox: BoundingBox) {
        self.cutouts.push(Cutout {
            bbox: cutout_bbox,
            shape: SubstrateLayerShape::Rect,
        });
    }

    pub fn add_cylinder_cutout(&mut self, cutout_bbox: BoundingBox, diameter: i64) {
        self.cutouts.push(Cutout {
            bbox: cutout_bbox,
            shape: SubstrateLayerShape::cylinder(diameter, 16),
        });
    }

    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        super::substrate_layer_contains::contains_nm(self, x, y, z)
    }

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

    pub fn is_axis_aligned_rectangle(&self) -> bool {
        self.cutouts.is_empty()
    }
}
