use crate::geometry::BoundingBox;
use compact_str::CompactString;

pub use crate::material::MaterialId;
pub use crate::netlist::NetId; // Use the strongly-typed NetId struct, not a raw u32 alias

#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub name: CompactString,
    pub position: crate::geometry::Point3D,
    pub material_id: MaterialId,
    pub net_id: Option<NetId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    None,
    Annular,
    Solid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Rotate90,
    Rotate180,
    Rotate270,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinerStack {
    pub liner_material: MaterialId,
    pub liner_thickness_nm: i64,
    pub bridge_material: Option<MaterialId>,
    pub bridge_thickness_nm: i64,
    pub fill_material: MaterialId,
}

impl LinerStack {
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TSVParams {
    pub diameter_nm: i64,
    pub stack: LinerStack,
    pub koz_multiplier: f32,
}

impl TSVParams {
    pub fn new(diameter_nm: i64, stack: LinerStack) -> Self {
        Self {
            diameter_nm,
            stack,
            koz_multiplier: 3.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cutout {
    pub bbox: BoundingBox,
    pub shape: super::SubstrateLayerShape,
}

pub struct TubeSpec {
    pub material: MaterialId,
    pub net: NetId,
    pub bbox: BoundingBox,
    pub outer_diameter: u32,
    pub inner_diameter: u32,
    pub pad_diameter: u32,
    pub segments: u32,
    pub top_cap: CapType,
    pub bottom_cap: CapType,
    pub bottom_outer_diameter: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionStats {
    pub total_slots: usize,
    pub allocated_chunks: usize,
    pub zombie_chunks: usize,
    pub active_chunks: usize,
    pub zombie_ratio: f64,
}
