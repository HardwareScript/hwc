//! The Entity Graph — master registry for all design entities.

mod component_pins;
mod drilling;
mod entity_registry;
mod impls;
mod routing;
mod spatial;
mod substrate;
mod types;

pub use types::{CapType, LinerStack, SubstrateLayerType, TSVParams};
pub use types::{EntityData, EntityType, TubeLayerSpec, ViaHoleSpec};

use crate::geometry::entity_ids::*;
use crate::geometry_router::scene_graph::SceneGraph;
use crate::geometry_router::spatial_index::DynamicSpatialIndex;
use crate::geometry_router::substrate_types::{ComponentMetadata, ComponentPin, SubstrateLayer};
use crate::netlist::{ComponentId, NetId, NetlistArena};
use rustc_hash::FxHashMap;

/// The Entity Graph — master registry for all design entities.
pub struct EntityGraph {
    /// Component/pin/net ECS arena (logical connectivity)
    pub(crate) netlist: NetlistArena,

    /// Component stamps and instances (physical geometry)
    pub(crate) scene: SceneGraph,

    /// R*-tree spatial index (dynamic, for floorplanning)
    pub(crate) spatial: DynamicSpatialIndex,

    /// Master registry for all routing-targetable entities (v0.1.8)
    pub(crate) entity_registry: FxHashMap<EntityId, EntityData>,

    /// Map from arena ComponentId to graph ComponentGraphId
    component_id_map: FxHashMap<ComponentId, ComponentGraphId>,

    /// Map from arena NetId to graph NetGraphId
    net_id_map: FxHashMap<NetId, NetGraphId>,

    /// Next stamp ID to assign
    _next_stamp_id: usize,

    /// Next instance ID to assign
    _next_instance_id: usize,

    /// Full SubstrateLayer objects.
    pub substrate_layers: Vec<SubstrateLayer>,
    /// Full ComponentMetadata objects.
    pub component_metadata: Vec<ComponentMetadata>,
    /// Full ComponentPin objects.
    pub component_pins: Vec<ComponentPin>,

    /// Canonical routed segments registered by the auto-router.
    pub(crate) routed_segments: Vec<(NetId, Vec<crate::geometry::TraceSegment>)>,
}

impl EntityGraph {
    /// Create a new empty Entity Graph.
    pub fn new() -> Self {
        Self {
            netlist: NetlistArena::new(),
            scene: SceneGraph::new(),
            spatial: DynamicSpatialIndex::new(),
            entity_registry: FxHashMap::default(),
            component_id_map: FxHashMap::default(),
            net_id_map: FxHashMap::default(),
            _next_stamp_id: 0,
            _next_instance_id: 0,
            substrate_layers: Vec::new(),
            component_metadata: Vec::new(),
            component_pins: Vec::new(),
            routed_segments: Vec::new(),
        }
    }

    /// Access the underlying NetlistArena (read-only).
    #[inline]
    pub fn netlist(&self) -> &NetlistArena {
        &self.netlist
    }

    /// Access the underlying NetlistArena (mutable).
    #[inline]
    pub fn netlist_mut(&mut self) -> &mut NetlistArena {
        &mut self.netlist
    }

    /// Access the underlying SceneGraph (read-only).
    #[inline]
    pub fn scene(&self) -> &SceneGraph {
        &self.scene
    }

    /// Access the underlying SceneGraph (mutable).
    #[inline]
    pub fn scene_mut(&mut self) -> &mut SceneGraph {
        &mut self.scene
    }

    /// Access the underlying DynamicSpatialIndex (read-only).
    #[inline]
    pub fn spatial(&self) -> &DynamicSpatialIndex {
        &self.spatial
    }

    /// Access the underlying DynamicSpatialIndex (mutable).
    #[inline]
    pub fn spatial_mut(&mut self) -> &mut DynamicSpatialIndex {
        &mut self.spatial
    }
}
