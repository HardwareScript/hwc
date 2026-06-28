use crate::geometry::BoundingBox;
use crate::geometry::entity_ids::*;
use crate::geometry_router::scene_graph::SceneGraph;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::substrate_types::{
    ComponentMetadata, ComponentPin, MaterialId,
    SubstrateLayer, SubstrateLayerShape,
};
use crate::netlist::{ComponentId, NetId, NetlistArena};
use rustc_hash::FxHashMap;

// Re-export substrate types
pub use crate::geometry_router::substrate_types::{CapType, LinerStack, TSVParams, SubstrateLayerType};

/// The Entity Graph — master registry for all design entities.
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

    /// Query: is the point (x, y, z) inside any component's physical geometry?
    #[inline]
    pub fn is_point_occupied(&self, x: i64, y: i64, _z: i64) -> bool {
        for inst in self.scene.instances() {
            if x < inst.global_bbox.min.x
                || x > inst.global_bbox.max.x
                || y < inst.global_bbox.min.y
                || y > inst.global_bbox.max.y
            {
                continue;
            }
            if inst.test_collision_global(x, y) {
                return true;
            }
        }
        false
    }

    /// Build the spatial index from all component instances in the scene graph.
    pub fn rebuild_spatial_index(&mut self) {
        self.spatial.clear();
        for inst in self.scene.instances() {
            let bbox = &inst.global_bbox;
            let segment = IndexedSegment {
                segment_id: inst.instance_id,
                net_id: inst.net_bindings.first().copied().unwrap_or(0),
                width_nm: bbox.max.x - bbox.min.x,
                thickness_nm: 35_000,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            };
            self.spatial.insert(segment);
        }
    }

    /// Get a reference to all component metadata.
    pub fn get_component_metadata(&self) -> &[ComponentMetadata] {
        &self.component_metadata
    }

    /// Query: Which component name is at this point?
    pub fn point_in_component(&self, x: i64, y: i64, z: i64) -> Option<compact_str::CompactString> {
        for meta in &self.component_metadata {
            if meta.bbox.contains(crate::geometry::Point3D::new(x, y, z)) {
                return Some(meta.name.clone());
            }
        }
        None
    }

    /// Query: Get the bounding box of a pour at this position.
    pub fn get_pour_bbox_at_position(&self, x: i64, y: i64, z: i64) -> Option<BoundingBox> {
        for layer in &self.substrate_layers {
            if layer.bbox.contains(crate::geometry::Point3D::new(x, y, z)) {
                return Some(layer.bbox);
            }
        }
        None
    }

    /// Add a full SubstrateLayer.
    pub fn add_substrate_layer(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
    ) {
        let layer = SubstrateLayer::new(material, net, bbox, layer_type);
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

    /// Add a cylindrical substrate layer (e.g. via pad).
    pub fn add_cylinder_substrate_layer(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        diameter_nm: i64,
        _segments: u32,
        _rotation_deg: i64,
    ) {
        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Pour);
        layer.shape = SubstrateLayerShape::Circle { radius: diameter_nm / 2 };
        // Note: SubstrateLayerShape::Circle doesn't store segments, but Polygon would.
        // For now we use Circle as defined in substrate_types.rs
        self.substrate_layers.push(layer);
    }

    /// Add a tube substrate layer (e.g. plated through-hole wall).
    pub fn add_tube_substrate_layer(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: CapType,
        bottom_cap: CapType,
        bottom_outer_diameter: Option<u32>,
    ) {
        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Contact);
        layer.shape = SubstrateLayerShape::Tube {
            outer_diameter,
            inner_diameter,
            pad_diameter,
            segments,
            top_cap,
            bottom_cap,
            bottom_outer_diameter,
        };
        self.substrate_layers.push(layer);
    }

    /// Add a polygonal substrate layer.
    pub fn add_polygon_substrate_layer(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        polygon: crate::geometry::Polygon,
    ) {
        let mut outer_contour = clipper2_rust::Path64::new();
        for p in &polygon.points {
            outer_contour.push(clipper2_rust::Point64::new(p.x, p.y));
        }

        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Pour);
        layer.shape = SubstrateLayerShape::Polygon {
            outer_contour,
            holes: clipper2_rust::Paths64::new(),
            segments: 32,
        };
        self.substrate_layers.push(layer);
    }

    /// Set the net for a specific component pin.
    pub fn set_pin_net(&mut self, component_name: &str, pin_name: &str, net_name: &str) {
        if let Some(pin) = self.component_pins.iter_mut().find(|p| {
            p.component_name.as_str() == component_name && p.pin_name.as_str() == pin_name
        }) {
            pin.net = Some(net_name.into());
        }
    }

    /// Add a circular substrate layer (alias for add_cylinder_substrate_layer for backward compatibility).
    pub fn add_circle_substrate_layer(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        radius_nm: i64,
    ) {
        self.add_cylinder_substrate_layer(material, net, bbox, radius_nm * 2, 32, 0);
    }

    /// Get all component pins.
    pub fn get_component_pins(&self) -> &[ComponentPin] {
        &self.component_pins
    }

    /// Get the bounding box of a pour associated with a pin.
    pub fn get_pour_bbox_for_pin(&self, component_name: &str, pin_name: &str) -> Option<BoundingBox> {
        // This is a simplified implementation. Real one might need more logic.
        self.component_pins
            .iter()
            .find(|p| p.component_name.as_str() == component_name && p.pin_name.as_str() == pin_name)
            .and_then(|p| self.get_pour_bbox_at_position(p.x_nm, p.y_nm, p.z_nm))
    }

    /// Commit the current routing session (placeholder).
    pub fn commit_route(&mut self) {
        // No-op for now as analytic routing is immediate
    }

    /// Add a TSV (Through Silicon Via) stack.
    pub fn add_tsv_stack(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
    ) {
        self.add_tube_substrate_layer(
            material,
            net,
            bbox,
            outer_diameter,
            inner_diameter,
            outer_diameter, // pad_diameter = outer_diameter for TSV
            32,             // segments
            crate::geometry_router::substrate_types::CapType::Solid,
            crate::geometry_router::substrate_types::CapType::Solid,
            None,
        );
    }

    /// Drill a hole through all substrate layers that intersect the given bbox.
    pub fn drill_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: Option<i64>,
        _drill_net: u32,
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
    pub fn drill_via_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: i64,
        via_net: u32,
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

    /// Register a routed path canonically as continuous vector segments.
    pub fn register_route(&mut self, net_id: NetId, waypoints: &[crate::geometry::Point3D]) {
        if waypoints.len() < 2 {
            return;
        }
        let segments: Vec<crate::geometry::TraceSegment> = waypoints
            .windows(2)
            .map(|w| {
                crate::geometry::TraceSegment::new(w[0], w[1], 200_000)
            })
            .collect();
        self.routed_segments.push((net_id, segments));
    }

    /// Get all canonically registered route segments across all nets.
    pub fn get_all_routes(&self) -> &[(NetId, Vec<crate::geometry::TraceSegment>)] {
        &self.routed_segments
    }

    /// Clear registered route segments for a specific net.
    pub fn clear_routes_for_net(&mut self, net_id: NetId) {
        self.routed_segments.retain(|(id, _)| *id != net_id);
    }

    /// Register a single point as occupied by a net (for polygon rasterization).
    pub fn occupy_point(
        &mut self,
        point: crate::geometry::Point3D,
        net_id: NetId,
        _material: MaterialId,
    ) {
        let segment = crate::geometry::TraceSegment::new(point, point, 0);
        if let Some(entry) = self.routed_segments.iter_mut().find(|(id, _)| *id == net_id) {
            entry.1.push(segment);
        } else {
            self.routed_segments.push((net_id, vec![segment]));
        }
    }

    /// Copy component metadata and pins from another EntityGraph.
    pub fn copy_metadata_from(&mut self, other: &EntityGraph) {
        self.component_metadata = other.component_metadata.clone();
        self.component_pins = other.component_pins.clone();
        self.substrate_layers = other.substrate_layers.clone();
        self.routed_segments = other.routed_segments.clone();
    }

    /// Convert substrate layers into IndexedSegments for spatial index insertion.
    pub fn get_substrate_layers_as_segments(&self) -> Vec<IndexedSegment> {
        self.substrate_layers
            .iter()
            .enumerate()
            .map(|(i, layer)| {
                let mut bboxes = vec![layer.bbox];
                for region in &layer.regions {
                    bboxes.push(*region);
                }
                let combined = bboxes.iter().fold(layer.bbox, |acc, b| acc.union(b));
                IndexedSegment {
                    segment_id: i,
                    net_id: layer.net as usize,
                    width_nm: combined.max.x - combined.min.x,
                    thickness_nm: combined.max.z - combined.min.z,
                    start: combined.min,
                    end: combined.max,
                    layer: combined.min.z,
                }
            })
            .collect()
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
            substrate_layers: self.substrate_layers.clone(),
            component_metadata: self.component_metadata.clone(),
            component_pins: self.component_pins.clone(),
            routed_segments: self.routed_segments.clone(),
        }
    }
}
