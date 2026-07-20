use crate::geometry::BoundingBox;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::core_types::{CardinalDirection, MaterialId, NetId, Terminal};

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentPin {
    pub x_nm: i64,
    pub y_nm: i64,
    pub z_nm: i64,
    pub component_name: CompactString,
    pub pin_name: CompactString,
    pub net: Option<CompactString>,
}

impl ComponentPin {
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

    pub fn position(&self) -> (i64, i64, i64) {
        (self.x_nm, self.y_nm, self.z_nm)
    }

    pub fn display_name(&self) -> CompactString {
        format!("{}.{}", self.component_name, self.pin_name).into()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMetadata {
    pub material: MaterialId,
    pub bbox: BoundingBox,
    pub name: CompactString,
    pub component_type: CompactString,
    pub terminals: Vec<Terminal>,
    pub net_bindings: FxHashMap<CompactString, NetId>,
    pub blocked_z_ranges: SmallVec<[(i64, i64); 2]>,
}

impl ComponentMetadata {
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

    pub fn add_terminal(&mut self, terminal: Terminal) {
        self.terminals.push(terminal);
    }

    pub fn bind_net(&mut self, pin_name: CompactString, net_id: NetId) {
        self.net_bindings.insert(pin_name, net_id);
    }

    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y
            && z >= self.bbox.min.z
            && z <= self.bbox.max.z
    }

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

    #[inline]
    pub fn has_material_on_z_range(&self, layer_z_min: i64, layer_z_max: i64) -> bool {
        self.bbox.min.z < layer_z_max && self.bbox.max.z > layer_z_min
    }

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
