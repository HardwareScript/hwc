//! Type definitions for the Entity Graph.

use crate::geometry::BoundingBox;
use crate::geometry_router::substrate_types::MaterialId;
use crate::netlist::NetId;

// Re-export substrate types for backward compatibility
pub use crate::geometry_router::substrate_types::{
    CapType, LinerStack, SubstrateLayerType, TSVParams,
};

/// Type of entity in the Entity Graph (v0.1.8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EntityType {
    ComponentPin,
    SpacePour,
    SubstrateRegion,
    MechanicalKeepout,
}

/// Metadata for an entity in the Entity Graph (v0.1.8)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityData {
    pub entity_type: EntityType,
    pub bbox: BoundingBox,
    pub net_id: Option<NetId>,
    pub name: compact_str::CompactString,
    pub layer_z: Option<i64>,
}

/// Builder-style specification for [`EntityGraph::add_tube_substrate_layer`].
#[derive(Clone, Debug)]
pub struct TubeLayerSpec {
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

impl TubeLayerSpec {
    /// Start building a [`TubeLayerSpec`] from the required fields.
    ///
    /// `circle_segments` is the PDK-declared fidelity for circular geometry
    /// (`manufacturing.circle_segments`). It is threaded through rather than
    /// defaulted so geometry generation and mesh export never disagree.
    pub fn builder(
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        circle_segments: u32,
    ) -> TubeLayerSpecBuilder {
        TubeLayerSpecBuilder {
            material,
            net,
            bbox,
            outer_diameter: 0,
            inner_diameter: 0,
            pad_diameter: 0,
            segments: circle_segments,
            top_cap: CapType::Solid,
            bottom_cap: CapType::Solid,
            bottom_outer_diameter: None,
        }
    }
}

/// Builder for [`TubeLayerSpec`].
pub struct TubeLayerSpecBuilder {
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
}

impl TubeLayerSpecBuilder {
    pub fn outer_diameter(mut self, v: u32) -> Self {
        self.outer_diameter = v;
        self
    }

    pub fn inner_diameter(mut self, v: u32) -> Self {
        self.inner_diameter = v;
        self
    }

    pub fn pad_diameter(mut self, v: u32) -> Self {
        self.pad_diameter = v;
        self
    }

    pub fn segments(mut self, v: u32) -> Self {
        self.segments = v;
        self
    }

    pub fn top_cap(mut self, v: CapType) -> Self {
        self.top_cap = v;
        self
    }

    pub fn bottom_cap(mut self, v: CapType) -> Self {
        self.bottom_cap = v;
        self
    }

    pub fn bottom_outer_diameter(mut self, v: Option<u32>) -> Self {
        self.bottom_outer_diameter = v;
        self
    }

    pub fn build(self) -> TubeLayerSpec {
        TubeLayerSpec {
            material: self.material,
            net: self.net,
            bbox: self.bbox,
            outer_diameter: self.outer_diameter,
            inner_diameter: self.inner_diameter,
            pad_diameter: self.pad_diameter,
            segments: self.segments,
            top_cap: self.top_cap,
            bottom_cap: self.bottom_cap,
            bottom_outer_diameter: self.bottom_outer_diameter,
        }
    }
}

/// Specification for [`EntityGraph::drill_via_hole`].
#[derive(Clone, Copy, Debug)]
pub struct ViaHoleSpec {
    pub hole_bbox: BoundingBox,
    pub diameter_nm: i64,
    pub via_net: NetId,
    pub clearance_nm: i64,
    pub is_tented: bool,
    pub pad_diameter_nm: i64,
}
