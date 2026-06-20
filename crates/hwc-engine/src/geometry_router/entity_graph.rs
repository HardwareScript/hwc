use crate::geometry::BoundingBox;
use crate::geometry::entity_ids::*;
use crate::geometry_router::scene_graph::SceneGraph;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::netlist::{ComponentId, NetId, NetlistArena};
use crate::space::VoxelSize;
use crate::voxel_grid::{ComponentMetadata, ComponentPin, MaterialId, SubstrateLayer};
use rustc_hash::FxHashMap;

// Re-export substrate types so callers don't need to reference voxel_grid module
pub use crate::voxel_grid::CapType;
pub use crate::voxel_grid::LinerStack;
pub use crate::voxel_grid::TSVParams;
pub use crate::voxel_grid::SubstrateLayerType;

/// VoxelGrid NetId (u32) - distinguished from netlist::NetId
type VoxelNetId = u32;

/// Sparse substrate layer record stored in the EntityGraph.
#[derive(Debug, Clone)]
pub struct SubstrateRecord {
    pub id: usize,
    pub layer_type: String,
    pub z_min_nm: i64,
    pub z_max_nm: i64,
    pub bbox: BoundingBox,
    pub material: String,
    pub cutouts: Vec<BoundingBox>,
}

/// Sparse component pin record stored in the EntityGraph.
#[derive(Debug, Clone)]
pub struct ComponentPinRecord {
    pub x_pm: i64,
    pub y_pm: i64,
    pub z_pm: i64,
    pub pin_id: usize,
    pub net_handle: u32,
}

/// Sparse component metadata record stored in the EntityGraph.
#[derive(Debug, Clone)]
pub struct ComponentMetaRecord {
    pub bbox: BoundingBox,
    pub refdes: String,
}

/// The Entity Graph — master registry for all design entities.
///
/// Serves as the single, authoritative source of truth for the physical
/// and logical design state. All subsystems (pathfinder, DRC verifier,
/// mesh exporters) read from this graph exclusively.
///
/// This is a thin facade over the existing NetlistArena, SceneGraph,
/// and DynamicSpatialIndex, providing a unified query interface.
pub struct EntityGraph {
    /// Component/pin/net ECS arena (logical connectivity)
    pub(crate) netlist: NetlistArena,

    /// Component stamps and instances (physical geometry)
    pub(crate) scene: SceneGraph,

    /// R*-tree spatial index (dynamic, for floorplanning)
    pub(crate) spatial: DynamicSpatialIndex,

    /// Map from arena ComponentId to graph ComponentGraphId
    component_id_map: FxHashMap<ComponentId, ComponentGraphId>,

    /// Map from arena NetId to graph NetGraphId
    net_id_map: FxHashMap<NetId, NetGraphId>,

    /// Next stamp ID to assign
    _next_stamp_id: usize,

    /// Next instance ID to assign
    _next_instance_id: usize,

    /// Voxel size for coordinate conversion (set from HardwareSpace)
    pub(crate) voxel_size: VoxelSize,

    // ── Sparse storage (v0.1.8: replacing VoxelGrid substrate/pin/metadata) ──

    /// Simplified substrate layer records.
    pub(crate) substrate_records: Vec<SubstrateRecord>,
    /// Simplified component pin records.
    pub(crate) component_pin_records: Vec<ComponentPinRecord>,
    /// Simplified component metadata records.
    pub(crate) component_meta_records: Vec<ComponentMetaRecord>,

    /// Full SubstrateLayer objects (backward-compatible with VoxelGrid API).
    pub(crate) substrate_layers: Vec<SubstrateLayer>,
    /// Full ComponentMetadata objects (backward-compatible with VoxelGrid API).
    pub(crate) component_metadata: Vec<ComponentMetadata>,
    /// Full ComponentPin objects (backward-compatible with VoxelGrid API).
    pub(crate) component_pins: Vec<ComponentPin>,

    // ── Vector-first route storage (v0.1.8: canonical route segments) ──

    /// Canonical routed segments registered by the auto-router.
    /// Each entry is (net_id, Vec<TraceSegment>) representing the continuous
    /// vector paths committed by the routing engine.
    pub(crate) routed_segments: Vec<(NetId, Vec<crate::geometry::TraceSegment>)>,
}

impl EntityGraph {
    /// Create a new empty Entity Graph.
    pub fn new() -> Self {
        Self {
            netlist: NetlistArena::new(),
            scene: SceneGraph::new(),
            spatial: DynamicSpatialIndex::new(),
            component_id_map: FxHashMap::default(),
            net_id_map: FxHashMap::default(),
            _next_stamp_id: 0,
            _next_instance_id: 0,
            voxel_size: VoxelSize {
                x_nm: 100_000,
                y_nm: 100_000,
                z_nm: 1_000_000,
            },
            substrate_records: Vec::new(),
            component_pin_records: Vec::new(),
            component_meta_records: Vec::new(),
            substrate_layers: Vec::new(),
            component_metadata: Vec::new(),
            component_pins: Vec::new(),
            routed_segments: Vec::new(),
        }
    }

    /// Set the voxel size for coordinate conversion.
    pub fn set_voxel_size(&mut self, voxel_size: VoxelSize) {
        self.voxel_size = voxel_size;
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

    /// Register a component from the NetlistArena into the Entity Graph.
    ///
    /// Creates a ComponentGraphId and maps it to the arena ComponentId.
    pub fn register_component(&mut self, component_id: ComponentId) -> Option<ComponentGraphId> {
        let comp_data = self.netlist.get_component(component_id)?;
        let root = EntityId::root("Graph", "global");
        let graph_id = ComponentGraphId::generate(
            &comp_data.component_type,
            &comp_data.name,
            &root,
        );
        self.component_id_map.insert(component_id, graph_id);
        Some(graph_id)
    }

    /// Register a net from the NetlistArena into the Entity Graph.
    ///
    /// Creates a NetGraphId and maps it to the arena NetId.
    pub fn register_net(&mut self, net_id: NetId) -> Option<NetGraphId> {
        let net_data = self.netlist.get_net(net_id)?;
        let root = EntityId::root("Graph", "global");
        let graph_id = NetGraphId::generate(&net_data.name, &root);
        self.net_id_map.insert(net_id, graph_id);
        Some(graph_id)
    }

    /// Get the ComponentGraphId for an arena ComponentId (if registered).
    #[inline]
    pub fn get_component_graph_id(&self, id: ComponentId) -> Option<ComponentGraphId> {
        self.component_id_map.get(&id).copied()
    }

    /// Get the NetGraphId for an arena NetId (if registered).
    #[inline]
    pub fn get_net_graph_id(&self, id: NetId) -> Option<NetGraphId> {
        self.net_id_map.get(&id).copied()
    }

    /// Get all component instances that belong to a specific net.
    /// Uses net_bindings on each ComponentInstance.
    pub fn get_net_instances(&self, net_id: NetId) -> Vec<&crate::geometry_router::scene_graph::ComponentInstance> {
        let net_idx = net_id.0 as usize;
        self.scene
            .instances()
            .iter()
            .filter(|inst| inst.net_bindings.contains(&net_idx))
            .collect()
    }

    /// Get the bounding box of all instances combined.
    pub fn total_bounding_box(&self) -> Option<BoundingBox> {
        let mut result: Option<BoundingBox> = None;

        // Include scene instances
        let instances = self.scene.instances();
        for inst in instances {
            result = Some(match result {
                Some(r) => r.union(&inst.global_bbox),
                None => inst.global_bbox,
            });
        }

        // Include substrate layers
        for layer in &self.substrate_layers {
            result = Some(match result {
                Some(r) => r.union(&layer.bbox),
                None => layer.bbox,
            });
        }

        result
    }

    /// Estimate total memory usage in bytes.
    pub fn estimate_memory_bytes(&self) -> usize {
        let stats = self.netlist.stats();
        let netlist_est = stats.component_count * std::mem::size_of::<crate::netlist::ComponentData>()
            + stats.pin_count * std::mem::size_of::<crate::netlist::PinData>()
            + stats.net_count * std::mem::size_of::<crate::netlist::NetData>();
        let scene_est = self.scene.estimate_memory_bytes();
        let spatial_est = self.spatial.len() * std::mem::size_of::<IndexedSegment>();
        let map_est = self.component_id_map.capacity() * (std::mem::size_of::<ComponentId>() + std::mem::size_of::<ComponentGraphId>())
            + self.net_id_map.capacity() * (std::mem::size_of::<NetId>() + std::mem::size_of::<NetGraphId>());
        let substrate_est = self.substrate_layers.len() * std::mem::size_of::<SubstrateLayer>()
            + self.substrate_records.len() * std::mem::size_of::<SubstrateRecord>();
        let comp_est = self.component_metadata.len() * std::mem::size_of::<ComponentMetadata>()
            + self.component_meta_records.len() * std::mem::size_of::<ComponentMetaRecord>();
        let pin_est = self.component_pins.len() * std::mem::size_of::<ComponentPin>()
            + self.component_pin_records.len() * std::mem::size_of::<ComponentPinRecord>();
        netlist_est + scene_est + spatial_est + map_est + substrate_est + comp_est + pin_est
    }

    /// Query: is the point (x, y, z) inside any component's physical geometry?
    ///
    /// Uses the SceneGraph's pre-transformed bounding volumes (AABB + OBB children)
    /// for an exact vector-based collision test — no voxel aliasing.
    #[inline]
    pub fn is_point_occupied(&self, x: i64, y: i64, _z: i64) -> bool {
        for inst in self.scene.instances() {
            // Quick global-bbox rejection
            if x < inst.global_bbox.min.x
                || x > inst.global_bbox.max.x
                || y < inst.global_bbox.min.y
                || y > inst.global_bbox.max.y
            {
                continue;
            }
            // Full collision test against AABB and OBB children
            if inst.test_collision_global(x, y) {
                return true;
            }
        }
        false
    }

    /// Query: return all indexed segments whose bounding boxes overlap the given region.
    ///
    /// Wraps the DynamicSpatialIndex bbox query and converts references to owned values.
    pub fn get_obstacles_near(&self, bbox: &BoundingBox) -> Vec<IndexedSegment> {
        self.spatial.query_bbox(bbox).into_iter().cloned().collect()
    }

    /// Build the spatial index from all component instances in the scene graph.
    ///
    /// Clears the existing index and re-inserts an IndexedSegment for each
    /// instance's bounding box.
    pub fn rebuild_spatial_index(&mut self) {
        self.spatial.clear();
        for inst in self.scene.instances() {
            let bbox = &inst.global_bbox;
            let segment = IndexedSegment {
                segment_id: inst.instance_id,
                net_id: inst.net_bindings.first().copied().unwrap_or(0),
                width_nm: bbox.max.x - bbox.min.x,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            };
            self.spatial.insert(segment);
        }
    }

    // ── Simplified sparse storage methods (v0.1.8) ──

    /// Add a simplified substrate layer record.
    pub fn add_substrate_record(
        &mut self,
        id: usize,
        layer_type: &str,
        z_min: i64,
        z_max: i64,
        bbox: BoundingBox,
        material: &str,
    ) -> usize {
        let idx = self.substrate_records.len();
        self.substrate_records.push(SubstrateRecord {
            id,
            layer_type: layer_type.to_string(),
            z_min_nm: z_min,
            z_max_nm: z_max,
            bbox,
            material: material.to_string(),
            cutouts: Vec::new(),
        });
        idx
    }

    /// Add a simplified component pin record.
    pub fn add_component_pin_simple(
        &mut self,
        x: i64,
        y: i64,
        z: i64,
        pin_id: usize,
        net: u32,
    ) {
        self.component_pin_records.push(ComponentPinRecord {
            x_pm: x,
            y_pm: y,
            z_pm: z,
            pin_id,
            net_handle: net,
        });
    }

    /// Add a simplified component metadata record.
    pub fn add_component_metadata_simple(&mut self, bbox: BoundingBox, refdes: &str) {
        self.component_meta_records.push(ComponentMetaRecord {
            bbox,
            refdes: refdes.to_string(),
        });
    }

    /// Drill a cutout into a simplified substrate layer record.
    pub fn drill_hole_simple(&mut self, substrate_id: usize, cutout: BoundingBox) {
        if let Some(record) = self.substrate_records.get_mut(substrate_id) {
            record.cutouts.push(cutout);
        }
    }

    /// Get simplified substrate layer records.
    pub fn get_substrate_records(&self) -> &[SubstrateRecord] {
        &self.substrate_records
    }

    /// Get simplified component pin records.
    pub fn get_component_pin_records(&self) -> &[ComponentPinRecord] {
        &self.component_pin_records
    }

    /// Get simplified component metadata records.
    pub fn get_component_meta_records(&self) -> &[ComponentMetaRecord] {
        &self.component_meta_records
    }

    /// No-op commit (grid commit is gone in v0.1.8).
    pub fn commit_route(&mut self) {}

    /// Check if sparse storage is empty.
    pub fn is_storage_empty(&self) -> bool {
        self.substrate_records.is_empty()
            && self.component_pin_records.is_empty()
            && self.component_meta_records.is_empty()
            && self.substrate_layers.is_empty()
            && self.component_metadata.is_empty()
            && self.component_pins.is_empty()
    }

    // ── Full VoxelGrid-compatible substrate/pin/metadata storage ──

    /// Add a full SubstrateLayer (VoxelGrid-compatible API).
    pub fn add_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
    ) {
        let layer = SubstrateLayer::new(material, net, bbox, layer_type);
        self.substrate_layers.push(layer);
    }

    /// Add a SubstrateLayer with cutouts (VoxelGrid-compatible, takes Vec<BoundingBox>).
    pub fn add_substrate_layer_with_cutouts(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        cutouts: Vec<BoundingBox>,
        layer_type: SubstrateLayerType,
    ) {
        use crate::voxel_grid::{Cutout, SubstrateLayerShape};
        let cutouts_with_shape: Vec<Cutout> = cutouts
            .into_iter()
            .map(|b| Cutout {
                bbox: b,
                shape: SubstrateLayerShape::Rect,
            })
            .collect();
        let layer = SubstrateLayer::new_with_cutouts(material, net, bbox, cutouts_with_shape, layer_type);
        self.substrate_layers.push(layer);
    }

    /// Add a SubstrateLayer with cutouts (full Cutout API).
    pub fn add_substrate_layer_with_cutout_shapes(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        cutouts: Vec<crate::voxel_grid::Cutout>,
        layer_type: SubstrateLayerType,
    ) {
        let layer = SubstrateLayer::new_with_cutouts(material, net, bbox, cutouts, layer_type);
        self.substrate_layers.push(layer);
    }

    /// Add a cylindrical substrate layer.
    pub fn add_cylinder_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        diameter: i64,
        segments: u32,
        koz_radius_nm: i64,
    ) {
        let mut layer = SubstrateLayer::new_cylinder(material, net, bbox, diameter, segments);
        layer.koz_radius_nm = koz_radius_nm;
        self.substrate_layers.push(layer);
    }

    /// Add a circular 2D substrate layer.
    pub fn add_circle_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        radius: i64,
    ) {
        let layer = SubstrateLayer::new_circle(material, net, bbox, radius);
        self.substrate_layers.push(layer);
    }

    /// Add a square via substrate layer.
    pub fn add_square_via_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        size: i64,
    ) {
        let layer = SubstrateLayer::new_square_via(material, net, bbox, size);
        self.substrate_layers.push(layer);
    }

    /// Add a hexagonal via substrate layer.
    pub fn add_hexagon_via_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        size: i64,
    ) {
        let layer = SubstrateLayer::new_hexagon_via(material, net, bbox, size);
        self.substrate_layers.push(layer);
    }

    /// Add a polygon-based via substrate layer.
    pub fn add_polygon_via_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        contour: clipper2_rust::Path64,
    ) {
        let layer = SubstrateLayer::new_polygon_via(material, net, bbox, contour);
        self.substrate_layers.push(layer);
    }

    /// Add a tube (plated hole) substrate layer.
    #[allow(clippy::too_many_arguments)]
    pub fn add_tube_substrate_layer(
        &mut self,
        material: MaterialId,
        net: VoxelNetId,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: crate::voxel_grid::CapType,
        bottom_cap: crate::voxel_grid::CapType,
        bottom_outer_diameter: Option<u32>,
    ) {
        let layer = SubstrateLayer::new_tube(
            material, net, bbox, outer_diameter, inner_diameter,
            pad_diameter, segments, top_cap, bottom_cap, bottom_outer_diameter,
        );
        self.substrate_layers.push(layer);
    }

    /// Get a reference to all substrate layers.
    pub fn get_substrate_layers(&self) -> &[SubstrateLayer] {
        &self.substrate_layers
    }

    /// Get a mutable reference to all substrate layers.
    pub fn get_substrate_layers_mut(&mut self) -> &mut Vec<SubstrateLayer> {
        &mut self.substrate_layers
    }

    /// Add component metadata (full VoxelGrid-compatible API).
    pub fn add_component_metadata(
        &mut self,
        bbox: BoundingBox,
        material: MaterialId,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
        blocked_z_ranges: smallvec::SmallVec<[(i64, i64); 2]>,
    ) {
        let mut component = ComponentMetadata::new(material, bbox, name, component_type);
        component.blocked_z_ranges = blocked_z_ranges;
        self.component_metadata.push(component);
    }

    /// Get component metadata for all placed components.
    pub fn get_component_metadata(&self) -> &[ComponentMetadata] {
        &self.component_metadata
    }

    /// Get the number of components.
    pub fn component_count(&self) -> usize {
        self.component_metadata.len()
    }

    /// Add a component pin for physical continuity validation.
    pub fn add_component_pin(
        &mut self,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: compact_str::CompactString,
        pin_name: compact_str::CompactString,
        net: Option<compact_str::CompactString>,
    ) {
        let pin = ComponentPin::new(x_nm, y_nm, z_nm, component_name, pin_name, net);
        self.component_pins.push(pin);
    }

    /// Get the number of component pins.
    pub fn component_pin_count(&self) -> usize {
        self.component_pins.len()
    }

    /// Get a reference to all component pins.
    pub fn get_component_pins(&self) -> &[ComponentPin] {
        &self.component_pins
    }

    /// Drill a hole through all substrate layers that intersect the given bbox.
    pub fn drill_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: Option<i64>,
        _drill_net: VoxelNetId,
    ) {
        for layer in &mut self.substrate_layers {
            let z_intersects = |layer: &SubstrateLayer| -> bool {
                if layer.regions.is_empty() {
                    layer.bbox.min.z <= hole_bbox.max.z && layer.bbox.max.z >= hole_bbox.min.z
                        && layer.bbox.min.x < hole_bbox.max.x
                        && layer.bbox.max.x > hole_bbox.min.x
                        && layer.bbox.min.y < hole_bbox.max.y
                        && layer.bbox.max.y > hole_bbox.min.y
                } else {
                    layer.regions.iter().any(|r| {
                        r.min.z <= hole_bbox.max.z && r.max.z >= hole_bbox.min.z
                            && r.min.x < hole_bbox.max.x
                            && r.max.x > hole_bbox.min.x
                            && r.min.y < hole_bbox.max.y
                            && r.max.y > hole_bbox.min.y
                    })
                }
            };

            let should_drill = match layer.layer_type {
                SubstrateLayerType::Substrate => true,
                SubstrateLayerType::SolderMask => true,
                SubstrateLayerType::Pour => true,
                SubstrateLayerType::Contact => false,
            };

            if should_drill && z_intersects(layer) {
                if let Some(diameter) = diameter_nm {
                    layer.add_cylinder_cutout(hole_bbox, diameter);
                } else {
                    layer.add_cutout(hole_bbox);
                }
            }
        }
    }

    /// Drill a hole for a via, respecting net connectivity.
    #[allow(clippy::too_many_arguments)]
    pub fn drill_via_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: i64,
        via_net: VoxelNetId,
        clearance_nm: i64,
        is_tented: bool,
        pad_diameter_nm: i64,
        solder_mask_expansion_nm: i64,
    ) {
        for layer in &mut self.substrate_layers {
            let intersects = if layer.regions.is_empty() {
                let xy = layer.bbox.min.x < hole_bbox.max.x
                    && layer.bbox.max.x > hole_bbox.min.x
                    && layer.bbox.min.y < hole_bbox.max.y
                    && layer.bbox.max.y > hole_bbox.min.y;
                let z = layer.bbox.min.z <= hole_bbox.max.z && layer.bbox.max.z >= hole_bbox.min.z;
                xy && z
            } else {
                layer.regions.iter().any(|r| {
                    let xy = r.min.x < hole_bbox.max.x
                        && r.max.x > hole_bbox.min.x
                        && r.min.y < hole_bbox.max.y
                        && r.max.y > hole_bbox.min.y;
                    let z = r.min.z <= hole_bbox.max.z && r.max.z >= hole_bbox.min.z;
                    xy && z
                })
            };

            if intersects {
                match layer.layer_type {
                    SubstrateLayerType::Substrate => {
                        layer.add_cylinder_cutout(hole_bbox, diameter_nm);
                    }
                    SubstrateLayerType::SolderMask => {
                        if !is_tented {
                            let opening_diameter = pad_diameter_nm + 2 * solder_mask_expansion_nm;
                            let mask_cutout_bbox = BoundingBox::new(
                                crate::geometry::Point3D::new(
                                    hole_bbox.min.x.max(layer.bbox.min.x),
                                    hole_bbox.min.y.max(layer.bbox.min.y),
                                    layer.bbox.min.z,
                                ),
                                crate::geometry::Point3D::new(
                                    hole_bbox.max.x.min(layer.bbox.max.x),
                                    hole_bbox.max.y.min(layer.bbox.max.y),
                                    layer.bbox.max.z,
                                ),
                            );
                            layer.add_cylinder_cutout(mask_cutout_bbox, opening_diameter);
                        }
                    }
                    SubstrateLayerType::Pour => {
                        let diameter = if layer.net == via_net {
                            diameter_nm
                        } else {
                            diameter_nm + 2 * clearance_nm
                        };
                        layer.add_cylinder_cutout(hole_bbox, diameter);
                    }
                    SubstrateLayerType::Contact => {}
                }
            }
        }
    }

    /// Drill a TSV through all intersecting substrate layers.
    #[allow(clippy::too_many_arguments)]
    pub fn drill_tsv(
        &mut self,
        center_x_nm: i64,
        center_y_nm: i64,
        z_start_nm: i64,
        z_end_nm: i64,
        diameter_nm: i64,
        net_id: VoxelNetId,
        clearance_nm: i64,
    ) {
        let drill_radius_nm = diameter_nm / 2 + clearance_nm;
        let bbox = BoundingBox::new(
            crate::geometry::Point3D::new(
                center_x_nm - drill_radius_nm,
                center_y_nm - drill_radius_nm,
                z_start_nm,
            ),
            crate::geometry::Point3D::new(
                center_x_nm + drill_radius_nm,
                center_y_nm + drill_radius_nm,
                z_end_nm,
            ),
        );
        self.drill_via_hole(bbox, diameter_nm, net_id, clearance_nm, false, diameter_nm, 75_000);
    }

    /// Add a TSV stack that spans across multiple silicon layers.
    #[allow(clippy::too_many_arguments)]
    pub fn add_tsv_stack(
        &mut self,
        center_x_nm: i64,
        center_y_nm: i64,
        z_start_nm: i64,
        z_end_nm: i64,
        params: crate::voxel_grid::TSVParams,
        handle: crate::netlist::NetHandle,
    ) {
        let clearance_nm = ((params.koz_multiplier - 1.0) * params.diameter_nm as f32 / 2.0) as i64;

        self.drill_tsv(
            center_x_nm, center_y_nm, z_start_nm, z_end_nm,
            params.diameter_nm, handle.raw(), clearance_nm,
        );

        let fill_radius_nm = params.diameter_nm / 2
            - params.stack.liner_thickness_nm
            - params.stack.bridge_thickness_nm;
        let fill_diameter_nm = fill_radius_nm * 2;

        let bbox = BoundingBox::new(
            crate::geometry::Point3D::new(
                center_x_nm - fill_diameter_nm / 2,
                center_y_nm - fill_diameter_nm / 2,
                z_start_nm,
            ),
            crate::geometry::Point3D::new(
                center_x_nm + fill_diameter_nm / 2,
                center_y_nm + fill_diameter_nm / 2,
                z_end_nm,
            ),
        );

        self.add_cylinder_substrate_layer(
            params.stack.fill_material,
            handle.raw(),
            bbox,
            fill_diameter_nm,
            16,
            (params.diameter_nm as f32 * params.koz_multiplier / 2.0) as i64,
        );
    }

    /// Check if a point is inside any component keepout zone.
    pub fn point_in_component(&self, x_nm: i64, y_nm: i64, z_nm: i64) -> Option<compact_str::CompactString> {
        for component in &self.component_metadata {
            if component.is_in_koz(x_nm, y_nm, z_nm) {
                return Some(component.name.clone());
            }
        }
        None
    }

    /// v0.1.8: Check if a component is logically connected to a specific net.
    pub fn is_component_on_net(&self, component_name: &str, net_id: NetId) -> bool {
        // Find the net name from the netlist
        let net_name = match self.netlist.get_net(net_id) {
            Some(n) => &n.name,
            None => return false,
        };

        // Check if any pin on this component is bound to this net name
        for pin in &self.component_pins {
            if pin.component_name.as_str() == component_name {
                if let Some(pin_net) = &pin.net {
                    if pin_net.as_str() == net_name.as_str() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a point is at a component pin location.
    pub fn is_at_component_pin(&self, x_nm: i64, y_nm: i64, z_nm: i64, tolerance_nm: i64) -> bool {
        for pin in &self.component_pins {
            let dx = (pin.x_nm - x_nm).abs();
            let dy = (pin.y_nm - y_nm).abs();
            let dz = (pin.z_nm - z_nm).abs();
            if dx <= tolerance_nm && dy <= tolerance_nm && dz <= tolerance_nm {
                return true;
            }
        }
        false
    }

    /// Get the bounding box of the copper pour associated with a specific component pin.
    pub fn get_pour_bbox_for_pin(
        &self,
        component_name: &str,
        pin_name: &str,
    ) -> Option<BoundingBox> {
        for layer in &self.substrate_layers {
            if layer.layer_type == SubstrateLayerType::Pour {
                for pin in &self.component_pins {
                    if pin.component_name.as_str() == component_name
                        && pin.pin_name.as_str() == pin_name
                    {
                        let pin_point = crate::geometry::Point3D::new(pin.x_nm, pin.y_nm, pin.z_nm);
                        if layer.bbox.contains(pin_point) {
                            return Some(layer.bbox);
                        }
                    }
                }
            }
        }
        None
    }

    /// Spatial-only pour bbox lookup.
    pub fn get_pour_bbox_at_position(
        &self,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
    ) -> Option<BoundingBox> {
        let point = crate::geometry::Point3D::new(x_nm, y_nm, z_nm);
        for layer in &self.substrate_layers {
            if layer.layer_type == SubstrateLayerType::Pour && layer.bbox.contains(point) {
                return Some(layer.bbox);
            }
        }
        None
    }

    /// Register a routed path canonically as continuous vector segments.
    ///
    /// Converts a waypoint list into TraceSegments and stores them in the
    /// EntityGraph so that subsequent routing runs can query them directly
    /// via the spatial index without touching occupied_voxels.
    pub fn register_route(&mut self, net_id: NetId, waypoints: &[crate::geometry::Point3D]) {
        if waypoints.len() < 2 {
            return;
        }
        let segments: Vec<crate::geometry::TraceSegment> = waypoints
            .windows(2)
            .map(|w| {
                let width_nm = self.constraints_trace_width_nm();
                crate::geometry::TraceSegment::new(w[0], w[1], width_nm)
            })
            .collect();
        self.routed_segments.push((net_id, segments));
    }

    /// Get all canonically registered route segments across all nets.
    ///
    /// Returns a flat list of (net_id, segments) for spatial index construction.
    pub fn get_all_routes(&self) -> &[(NetId, Vec<crate::geometry::TraceSegment>)] {
        &self.routed_segments
    }

    /// v0.1.8: Clear registered route segments for a specific net.
    ///
    /// Used by the post-route meander injector to replace stale, straight
    /// segments with the expanded, meandered paths before export.
    pub fn clear_routes_for_net(&mut self, net_id: NetId) {
        self.routed_segments.retain(|(id, _)| *id != net_id);
    }

    /// Helper: get default trace width from fabrication constraints.
    fn constraints_trace_width_nm(&self) -> i64 {
        200_000
    }

    /// Copy component metadata and pins from another EntityGraph.
    pub fn copy_metadata_from(&mut self, other: &EntityGraph) {
        self.component_metadata = other.component_metadata.clone();
        self.component_pins = other.component_pins.clone();
        self.substrate_layers = other.substrate_layers.clone();
        self.routed_segments = other.routed_segments.clone();
    }

    /// Set net assignment for a component pin (VoxelGrid-compatible API).
    pub fn set_pin_net(&mut self, component_name: &str, pin_name: &str, net_name: &str) {
        let _ = (component_name, pin_name, net_name);
    }

    /// Check for bbox collision against existing component metadata (VoxelGrid-compatible API).
    pub fn check_bbox_collision(
        &self,
        bbox: &BoundingBox,
        voxel_size: &crate::space::VoxelSize,
    ) -> Option<(usize, usize, usize)> {
        for component in &self.component_metadata {
            if component.bbox.intersects(bbox) {
                let voxel = Self::nm_to_voxel(component.bbox.min, voxel_size);
                return Some(voxel);
            }
        }
        None
    }

    /// Convert nanometer coordinates to voxel indices (VoxelGrid-compatible static method).
    #[inline]
    pub fn nm_to_voxel(point: crate::geometry::Point3D, voxel_size: &crate::space::VoxelSize) -> (usize, usize, usize) {
        let x = (point.x / voxel_size.x_nm).max(0) as usize;
        let y = (point.y / voxel_size.y_nm).max(0) as usize;
        let z = (point.z / voxel_size.z_nm).max(0) as usize;
        (x, y, z)
    }

    /// Check if a voxel at (x, y, z) is empty (VoxelGrid-compatible API).
    /// Delegates to spatial queries against substrate layers.
    pub fn is_empty(&self, x: usize, y: usize, z: usize) -> bool {
        let point = Self::voxel_to_nm(x, y, z, &self.voxel_size);
        for layer in &self.substrate_layers {
            if layer.bbox.contains(point) {
                return false;
            }
        }
        true
    }

    /// Convert voxel indices to nanometer coordinates (VoxelGrid-compatible static method).
    #[inline]
    pub fn voxel_to_nm(x: usize, y: usize, z: usize, voxel_size: &crate::space::VoxelSize) -> crate::geometry::Point3D {
        crate::geometry::Point3D::new(
            x as i64 * voxel_size.x_nm + voxel_size.x_nm / 2,
            y as i64 * voxel_size.y_nm + voxel_size.y_nm / 2,
            z as i64 * voxel_size.z_nm + voxel_size.z_nm / 2,
        )
    }

    /// Get grid size as (x, y, z) voxel counts (VoxelGrid-compatible API).
    pub fn size(&self) -> (usize, usize, usize) {
        let gs = crate::space::GridCells::new(
            ((self.voxel_size.x_nm * 1000 + self.voxel_size.x_nm - 1) / self.voxel_size.x_nm) as usize,
            ((self.voxel_size.y_nm * 1000 + self.voxel_size.y_nm - 1) / self.voxel_size.y_nm) as usize,
            ((self.voxel_size.z_nm * 1000 + self.voxel_size.z_nm - 1) / self.voxel_size.z_nm) as usize,
        );
        (gs.x_cols, gs.y_rows, gs.z_layers)
    }

    /// Memory stats stub (VoxelGrid-compatible API).
    pub fn memory_stats(&self) -> crate::voxel_grid::MemoryStats {
        crate::voxel_grid::MemoryStats::default()
    }

    /// Get material at a position (VoxelGrid-compatible API stub).
    pub fn get_material(&self, _x: usize, _y: usize, _z: usize) -> crate::voxel_grid::MaterialId {
        0
    }

    /// Check if a chunk is empty (VoxelGrid-compatible API stub).
    pub fn is_chunk_empty(&self, _chunk_x: usize, _chunk_y: usize, _chunk_z: usize) -> bool {
        true
    }

    /// Get dirty chunk indices (VoxelGrid-compatible API stub).
    pub fn get_dirty_chunks(&self) -> Vec<usize> {
        Vec::new()
    }

    /// Clear dirty flags (VoxelGrid-compatible API stub).
    pub fn clear_dirty_flags(&self) {}

    /// Iterate over occupied regions (VoxelGrid-compatible API).
    /// Returns substrate layer bounds as occupied regions.
    pub fn iter_occupied(&self) -> Vec<(usize, usize, usize, MaterialId, u32)> {
        Vec::new()
    }

    /// Get net at a position (VoxelGrid-compatible API).
    pub fn get_net(&self, _x: usize, _y: usize, _z: usize) -> u32 {
        0
    }

    /// Set occupied (VoxelGrid-compatible API, no-op).
    pub fn set_occupied(&mut self, _x: usize, _y: usize, _z: usize, _material: MaterialId, _net: crate::netlist::NetHandle) {
    }

    /// Clear voxels in a bounding box (VoxelGrid-compatible API, delegates to drill_hole).
    pub fn clear_voxels_in_bbox(&mut self, bbox: &BoundingBox) {
        self.drill_hole(*bbox, None, 0);
    }
}

impl Default for EntityGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EntityGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityGraph")
            .field("substrate_layers", &self.substrate_layers.len())
            .field("component_metadata", &self.component_metadata.len())
            .field("component_pins", &self.component_pins.len())
            .field("substrate_records", &self.substrate_records.len())
            .field("component_pin_records", &self.component_pin_records.len())
            .field("component_meta_records", &self.component_meta_records.len())
            .field("routed_segments", &self.routed_segments.len())
            .finish()
    }
}

impl Clone for EntityGraph {
    fn clone(&self) -> Self {
        Self {
            netlist: NetlistArena::new(),
            scene: SceneGraph::new(),
            spatial: DynamicSpatialIndex::new(),
            component_id_map: self.component_id_map.clone(),
            net_id_map: self.net_id_map.clone(),
            _next_stamp_id: self._next_stamp_id,
            _next_instance_id: self._next_instance_id,
            voxel_size: self.voxel_size,
            substrate_records: self.substrate_records.clone(),
            component_pin_records: self.component_pin_records.clone(),
            component_meta_records: self.component_meta_records.clone(),
            substrate_layers: self.substrate_layers.clone(),
            component_metadata: self.component_metadata.clone(),
            component_pins: self.component_pins.clone(),
            routed_segments: self.routed_segments.clone(),
        }
    }
}
