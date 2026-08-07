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
use crate::geometry_router::connection_interface::{InterfaceId, PhysicalInterface};
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

    // ── v0.1.9: Connection Interface Routing (CIR) ──
    /// Dense arena of physical interfaces, indexed by InterfaceId.
    interface_database: FxHashMap<InterfaceId, PhysicalInterface>,

    /// Maps entity names (space entities or component.pin) to their interface IDs
    /// This allows quick interface lookup during routing without needing ComponentId
    entity_interface_map: FxHashMap<compact_str::CompactString, InterfaceId>,

    /// Next interface ID to allocate.
    next_interface_id: u32,

    /// Maps ComponentId to its list of interface IDs.
    component_interfaces: FxHashMap<ComponentId, Vec<InterfaceId>>,

    /// Maps (ComponentId, pin_name) to interface IDs for quick lookup.
    pin_interface_map: FxHashMap<(ComponentId, compact_str::CompactString), Vec<InterfaceId>>,
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
            interface_database: FxHashMap::default(),
            entity_interface_map: FxHashMap::default(),
            next_interface_id: 0,
            component_interfaces: FxHashMap::default(),
            pin_interface_map: FxHashMap::default(),
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

    // ── v0.1.9: Connection Interface Management ──

    /// Allocate a new unique InterfaceId.
    pub fn allocate_interface_id(&mut self) -> InterfaceId {
        let id = InterfaceId::new(self.next_interface_id);
        self.next_interface_id += 1;
        id
    }

    /// Register a physical interface on a component.
    ///
    /// Allocates a new `InterfaceId`, stores the interface in the database,
    /// and links it to the component and optional pin name.
    pub fn register_interface(
        &mut self,
        component_id: ComponentId,
        interface: PhysicalInterface,
    ) -> InterfaceId {
        let id = interface.id;
        self.interface_database.insert(id, interface);
        self.component_interfaces
            .entry(component_id)
            .or_default()
            .push(id);
        id
    }

    /// Register a physical interface with a pin name association.
    pub fn register_interface_with_pin(
        &mut self,
        component_id: ComponentId,
        pin_name: compact_str::CompactString,
        interface: PhysicalInterface,
    ) -> InterfaceId {
        let id = self.register_interface(component_id, interface);
        self.pin_interface_map
            .entry((component_id, pin_name))
            .or_default()
            .push(id);
        id
    }

    /// Look up a physical interface by its ID.
    #[inline]
    pub fn get_interface(&self, id: InterfaceId) -> Option<&PhysicalInterface> {
        self.interface_database.get(&id)
    }

    /// Get all interface IDs for a component.
    #[inline]
    pub fn get_component_interfaces(&self, component_id: ComponentId) -> &[InterfaceId] {
        self.component_interfaces
            .get(&component_id)
            .map_or(&[], |v| v.as_slice())
    }

    /// Get interface IDs associated with a specific pin on a component.
    pub fn get_pin_interfaces(&self, component_id: ComponentId, pin_name: &str) -> &[InterfaceId] {
        self.pin_interface_map
            .get(&(component_id, compact_str::CompactString::new(pin_name)))
            .map_or(&[], |v| v.as_slice())
    }

    /// Get interface by entity name (space entity or component.pin)
    pub fn get_interface_by_entity_name(&self, entity_name: &str) -> Option<&PhysicalInterface> {
        self.entity_interface_map
            .get(entity_name)
            .and_then(|id| self.interface_database.get(id))
    }

    /// Register interface for a space entity (pad/plane/pour) by name
    pub fn register_space_entity_interface(
        &mut self,
        entity_name: impl Into<compact_str::CompactString>,
        interface: PhysicalInterface,
    ) -> InterfaceId {
        let id = interface.id;
        let name: compact_str::CompactString = entity_name.into();
        self.interface_database.insert(id, interface);
        self.entity_interface_map.insert(name, id);
        id
    }

    /// Get all registered interfaces.
    pub fn all_interfaces(&self) -> impl Iterator<Item = &PhysicalInterface> {
        self.interface_database.values()
    }

    /// Total number of registered interfaces.
    #[inline]
    pub fn interface_count(&self) -> usize {
        self.interface_database.len()
    }

    /// Get read-only access to routed segments.
    #[inline]
    pub fn routed_segments(&self) -> &[(NetId, Vec<crate::geometry::TraceSegment>)] {
        &self.routed_segments
    }

    /// Get the count of routed segment groups.
    #[inline]
    pub fn routed_segment_count(&self) -> usize {
        self.routed_segments.len()
    }

    /// Get mutable access to routed segments.
    #[inline]
    pub fn routed_segments_mut(&mut self) -> &mut Vec<(NetId, Vec<crate::geometry::TraceSegment>)> {
        &mut self.routed_segments
    }

    /// Iterate over routed segments (net ID and segment list pairs).
    #[inline]
    pub fn iter_routed_segments(
        &self,
    ) -> impl Iterator<Item = (&NetId, &Vec<crate::geometry::TraceSegment>)> {
        self.routed_segments
            .iter()
            .map(|(net_id, segments)| (net_id, segments))
    }

    /// Add routed segments for a net.
    pub fn add_routed_segments(
        &mut self,
        net_id: NetId,
        segments: Vec<crate::geometry::TraceSegment>,
    ) {
        self.routed_segments.push((net_id, segments));
    }
}
