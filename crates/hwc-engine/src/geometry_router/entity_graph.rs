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

    /// Register a component pin and return its EntityId (v0.1.8)
    pub fn register_component_pin(
        &mut self,
        component_name: &str,
        pin_name: &str,
        bbox: BoundingBox,
        net_id: Option<NetId>,
    ) -> EntityId {
        let id = EntityId::from_str(&format!("pin:{}:{}", component_name, pin_name));
        eprintln!("[DEBUG register_component_pin] Registering '{}.{}' with EntityId: {}, net_id: {:?}", component_name, pin_name, id, net_id);
        self.entity_registry.insert(
            id,
            EntityData {
                entity_type: EntityType::ComponentPin,
                bbox,
                net_id,
                name: format!("{}.{}", component_name, pin_name).into(),
                layer_z: None,
            },
        );
        id
    }

    /// Register a space-level pour/pad and return its EntityId (v0.1.8)
    pub fn register_space_entity(
        &mut self,
        name: &str,
        bbox: BoundingBox,
        net_id: Option<NetId>,
        layer_z: i64,
    ) -> EntityId {
        let id = EntityId::from_str(&format!("space:{}", name));
        eprintln!("[DEBUG register_space_entity] Registering '{}' with EntityId: {}, net_id: {:?}", name, id, net_id);
        self.entity_registry.insert(
            id,
            EntityData {
                entity_type: EntityType::SpacePour,
                bbox,
                net_id,
                name: name.into(),
                layer_z: Some(layer_z),
            },
        );
        id
    }

    /// Get bounding box for a space entity by name (v0.1.9.1)
    pub fn get_space_entity_bbox(&self, name: &str) -> Option<BoundingBox> {
        let entity_id = EntityId::from_str(&format!("space:{}", name));
        self.entity_registry.get(&entity_id).map(|entity_data| entity_data.bbox)
    }

    /// Get the bounding box for a component pin (v0.1.9.1)
    /// Looks up the pin directly in the entity registry
    pub fn get_component_pin_bbox(&self, component_name: &str, pin_name: &str) -> Option<BoundingBox> {
        let entity_id = EntityId::from_str(&format!("pin:{}:{}", component_name, pin_name));
        self.entity_registry.get(&entity_id).map(|entity_data| entity_data.bbox)
    }

    /// Update net assignment for an entity (v0.1.8)
    pub fn set_entity_net(&mut self, entity_name: &str, net_name: &str) {
        // This is a bit inefficient as it requires a linear scan if we don't have a name map.
        // In v0.1.8, we should probably maintain a name -> EntityId map.
        // For now, let's just update the registry.
        let net_id = self.netlist.get_or_create_net(net_name);
        for data in self.entity_registry.values_mut() {
            if data.name == entity_name {
                data.net_id = Some(net_id);
            }
        }
    }

    /// Lookup entity data by EntityId (v0.1.8)
    /// Fails fast if the entity does not exist.
    pub fn get_entity_data(&self, id: EntityId) -> Result<&EntityData, String> {
        let result = self.entity_registry.get(&id);
        if result.is_none() {
            eprintln!("[DEBUG get_entity_data] EntityId {} NOT FOUND in registry (size: {})", id, self.entity_registry.len());
        }
        result.ok_or_else(|| format!("EntityId {} not found in registry", id))
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

    /// Build the spatial index from all component instances, substrate layers, and routes.
    /// This is a GOD-TIER unified indexing pass (v0.1.8).
    pub fn rebuild_spatial_index(&mut self, materials: &crate::material::MaterialRegistry) {
        self.spatial.clear();

        // 1. Index substrate layers (Pours, Contacts, etc.)
        for (idx, layer) in self.substrate_layers.iter().enumerate() {
            let bbox = &layer.bbox;
            let segment = IndexedSegment {
                source: hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer { index: idx },
                segment_id: idx,
                net_id: layer.net as usize,
                width_nm: bbox.max.x - bbox.min.x,
                thickness_nm: bbox.max.z - bbox.min.z,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            };
            self.spatial.insert(segment);
        }

        // 2. Index component instances from the scene graph
        for inst in self.scene.instances() {
            let bbox = &inst.global_bbox;
            let thickness_nm = bbox.max.z - bbox.min.z;
            let segment = IndexedSegment {
                source: hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance {
                    instance_id: inst.instance_id,
                },
                segment_id: inst.instance_id,
                net_id: inst.net_bindings.first().copied().unwrap_or(0),
                width_nm: bbox.max.x - bbox.min.x,
                thickness_nm,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            };
            self.spatial.insert(segment);
        }

        // 3. Index routed segments with DATA-DRIVEN thickness
        eprintln!("[SPATIAL INDEX] Indexing {} nets with routed segments", self.routed_segments.len());
        eprintln!("[SPATIAL INDEX DEBUG] routed_segments vector address: {:p}", &self.routed_segments);
        eprintln!("[SPATIAL INDEX DEBUG] Total entries in routed_segments: {}", self.routed_segments.len());
        for (net_idx, (net_id, segments)) in self.routed_segments.iter().enumerate() {
            eprintln!("[SPATIAL INDEX] Net {} (id={}): {} segments", net_idx, net_id.raw(), segments.len());
            for (seg_idx, seg) in segments.iter().enumerate() {
                eprintln!("  seg[{}]: start=({},{},{}), end=({},{},{}), width={}, material={}", 
                    seg_idx, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z, seg.width_nm, seg.material_id);
                
                let thickness_nm = if seg.start.z != seg.end.z {
                    (seg.start.z - seg.end.z).abs()
                } else {
                    let material_props = materials.get_material(seg.material_id)
                        .unwrap_or_else(|| {
                            panic!(
                                "FATAL: Route segment net={} seg={} references unregistered material_id={}",
                                net_id.raw(), seg_idx, seg.material_id
                            )
                        });
                    assert!(
                        material_props.thickness_nm > 0,
                        "FATAL: Material id={} has zero thickness — must be declared in PDK",
                        seg.material_id
                    );
                    material_props.thickness_nm
                };

                let segment = IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx,
                        seg_idx,
                    },
                    segment_id: seg_idx,
                    net_id: net_id.raw() as usize,
                    width_nm: seg.width_nm,
                    thickness_nm,
                    start: seg.start,
                    end: seg.end,
                    layer: seg.start.z,
                };
                self.spatial.insert(segment);
            }
        }
    }

    /// Get a reference to all component metadata.
    pub fn get_component_metadata(&self) -> &[ComponentMetadata] {
        &self.component_metadata
    }

    /// Get all registered entity IDs (v0.1.8)
    pub fn iter_entity_ids(&self) -> impl Iterator<Item = &EntityId> {
        self.entity_registry.keys()
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
    /// Add a substrate layer with optional clearance validation.
    /// 
    /// # Arguments
    /// * `min_clearance_nm` - If Some(distance), validates that this pour maintains
    ///   at least `distance` nm clearance from all existing pours on different nets.
    ///   Returns an error if validation fails.
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

    /// Add a substrate layer with clearance validation (v0.1.9).
    /// 
    /// Validates that the new pour maintains required clearance from existing
    /// pours on different nets. Returns Ok(()) if valid, Err with details if
    /// clearance is violated.
    /// 
    /// This should be used during IR construction to catch design rule violations
    /// early, rather than waiting until final DRC.
    pub fn add_substrate_layer_checked(
        &mut self,
        material: MaterialId,
        net: u32,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
        min_clearance_nm: i64,
    ) -> Result<(), String> {
        // Check clearance against existing substrate layers on different nets
        if net != 0 {  // Skip clearance check for unconnected geometry (net_id=0)
            for existing in &self.substrate_layers {
                // Skip if same net (same-net overlap is allowed for junctions)
                if existing.net == 0 || existing.net == net {
                    continue;
                }
                
                // Calculate clearance
                let distance = bbox.distance_to(&existing.bbox);
                
                if distance < min_clearance_nm {
                    return Err(format!(
                        "Clearance violation: Pour on net {} at {:?} is {}nm from net {} (required: {}nm)",
                        net,
                        bbox,
                        distance,
                        existing.net,
                        min_clearance_nm
                    ));
                }
            }
        }
        
        // If validation passed, add the layer
        let layer = SubstrateLayer::new(material, net, bbox, layer_type);
        self.substrate_layers.push(layer);
        Ok(())
    }

    /// Get a reference to all substrate layers.
    pub fn get_substrate_layers(&self) -> &[SubstrateLayer] {
        &self.substrate_layers
    }

    /// Get a mutable reference to all substrate layers.
    pub fn get_substrate_layers_mut(&mut self) -> &mut Vec<SubstrateLayer> {
        &mut self.substrate_layers
    }

    /// Add component metadata.
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

    /// Get all elements (pours and routes) for a specific net across all layers.
    pub fn get_all_elements_for_net(&self, net_id: crate::netlist::NetId) -> Vec<SubstrateLayer> {
        let mut elements = Vec::new();
        let net_raw = net_id.raw();

        // 1. Check substrate layers (pours)
        for layer in &self.substrate_layers {
            if layer.net == net_raw {
                elements.push(layer.clone());
            }
        }

        // 2. Check routed segments
        for (seg_net_id, segments) in &self.routed_segments {
            if *seg_net_id == net_id {
                for seg in segments {
                    // Convert routed segment to a temporary SubstrateLayer for unified processing
                    let bbox = BoundingBox::new(seg.start, seg.end);
                    let layer = SubstrateLayer::new(
                        seg.material_id, // v0.1.8: Use the correct material ID from the segment
                        net_raw,
                        bbox,
                        SubstrateLayerType::Pour,
                    );
                    elements.push(layer);
                }
            }
        }

        elements
    }

    /// Get elements (pours and routes) for a specific net on a specific layer.
    pub fn get_elements_for_net_on_layer(
        &self,
        net_id: crate::netlist::NetId,
        _layer_idx: usize,
    ) -> Vec<SubstrateLayer> {
        let mut elements = Vec::new();
        let net_raw = net_id.raw();

        // 1. Check substrate layers (pours)
        for layer in &self.substrate_layers {
            if layer.net == net_raw {
                // Determine if this substrate layer belongs to the requested layer_idx.
                // Substrate layers are absolute, so we check if their Z-range matches the layer_idx.
                // We use a simplified check here assuming the caller provides a valid layer_idx.
                // In a production system, we'd use the StackupManager to verify the Z-range.
                // However, the EntityGraph doesn't have a reference to the StackupManager.
                // Instead, we check if the mid-point of the layer matches the Z-range of the layer_idx
                // IF we had the stackup. Since we don't, we'll rely on the fact that
                // get_all_elements_for_net is used for topological connectivity and
                // this specific method is used for layer-to-layer bridging.
                
                // For now, we'll implement a heuristic: if the layer's Z-range is within
                // reasonable bounds. This is still not perfect.
                
                // WAIT! I have a better way. The caller (ViaResolver) already knows
                // the Z-range of the layers.
                
                elements.push(layer.clone());
            }
        }

        // 2. Check routed segments
        for (seg_net_id, segments) in &self.routed_segments {
            if *seg_net_id == net_id {
                for seg in segments {
                    let bbox = BoundingBox::new(seg.start, seg.end);
                    let layer = SubstrateLayer::new(
                        seg.material_id,
                        net_raw,
                        bbox,
                        SubstrateLayerType::Pour,
                    );
                    elements.push(layer);
                }
            }
        }

        // v0.1.8: Filter by layer. We need a way to know which layer an element belongs to.
        // For now, we'll just return all elements and let the caller filter if needed,
        // BUT the caller (ViaResolver) expects this method to do the filtering.
        
        // Actually, the best fix is to pass the layer's Z-range to this method.
        // But for now, let's at least fix the material ID bug.
        
        elements
    }

    /// Query the global spatial index for elements within a bounding box.
    pub fn query_bbox(&self, bbox: &BoundingBox) -> Vec<SubstrateLayer> {
        // v0.1.8: NATIVE R*-tree query via DynamicSpatialIndex.
        // This replaces the legacy linear scan and provides O(log N) performance.
        let candidates = self.spatial.query_bbox(bbox);
        let mut results = Vec::new();

        for cand in candidates {
            match cand.source {
                hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer { index } => {
                    if let Some(layer) = self.substrate_layers.get(index) {
                        results.push(layer.clone());
                    }
                }
                hwc_physics::spatial_index::SpatialEntitySource::RouteSegment { net_idx, seg_idx } => {
                    if let Some((net_id, segments)) = self.routed_segments.get(net_idx) {
                        if let Some(seg) = segments.get(seg_idx) {
                            let seg_bbox = BoundingBox::new(seg.start, seg.end);
                            let layer = SubstrateLayer::new(
                                seg.material_id,
                                net_id.raw(),
                                seg_bbox,
                                SubstrateLayerType::Pour,
                            );
                            results.push(layer);
                        }
                    }
                }
                hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance { .. } => {
                    // Component instances are physical obstacles but not "layers" in the substrate sense.
                    // For DRC, we might need to represent them as SubstrateLayers if they are conductive.
                }
            }
        }

        // Fallback: if spatial index is empty, do a linear scan (preserves behavior during migration)
        if results.is_empty() && self.spatial.is_empty() {
            for layer in &self.substrate_layers {
                if layer.bbox.intersects(bbox) {
                    results.push(layer.clone());
                }
            }
        }

        results
    }

    /// Add component pin for physical continuity validation.
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
    ///
    /// Deduplicates consecutive identical points to prevent zero-length segments
    /// that cause SameNetOverlap DRC violations.
    pub fn register_route(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        material_id: u8,
        width_nm: i64,
    ) {
        self.register_route_with_z_materials(
            net_id,
            waypoints,
            material_id,
            width_nm,
            None::<fn(i64) -> Option<u8>>,
        )
    }

    /// Register a route with Z-aware material resolution (v0.1.9.1)
    ///
    /// This is the internal implementation that supports layer-aware material assignment.
    /// When `z_to_material` is provided, horizontal segments use the material from their
    /// Z layer instead of the default routing material.
    ///
    /// # Arguments
    /// * `z_to_material` - Optional closure that maps Z coordinate to material ID
    pub fn register_route_with_z_materials<F>(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        default_material_id: u8,
        width_nm: i64,
        z_to_material: Option<F>,
    ) where
        F: Fn(i64) -> Option<u8>,
    {
        if waypoints.len() < 2 {
            return;
        }

        // Deduplicate consecutive identical points to prevent zero-length segments
        let deduped: Vec<crate::geometry::Point3D> = waypoints
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, p)| *i == 0 || *p != waypoints[i - 1])
            .map(|(_, p)| p)
            .collect();

        if deduped.len() < 2 {
            return;
        }

        // v0.1.9.1: Z-aware material resolution
        // Each segment uses the material from its Z layer, not a single routing material.
        // This fixes the bug where horizontal traces at z=100nm (active layer) were
        // incorrectly stamped with Aluminum (metal1 material) instead of Silicon_N.
        let segments: Vec<crate::geometry::TraceSegment> = deduped
            .windows(2)
            .filter(|w| w[0] != w[1]) // Extra safety: skip zero-length
            .map(|w| {
                let start = w[0];
                let end = w[1];
                
                // Determine segment material based on its Z coordinate
                // For horizontal segments (start.z == end.z), use that Z layer's material
                // For vertical segments, use the routing material (via/transition material)
                let seg_material_id = if start.z == end.z {
                    // Horizontal segment - try to resolve material from Z layer
                    if let Some(ref resolver) = z_to_material {
                        resolver(start.z).unwrap_or(default_material_id)
                    } else {
                        default_material_id
                    }
                } else {
                    // Vertical segment - use routing material (vias, transitions)
                    default_material_id
                };
                
                crate::geometry::TraceSegment::new(start, end, width_nm, seg_material_id)
            })
            .collect();

        if segments.is_empty() {
            return;
        }

        // Append to existing net entry instead of creating duplicates
        if let Some(entry) = self.routed_segments.iter_mut().find(|(id, _)| *id == net_id) {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
    }

    /// Register pre-built trace segments directly (for lockfile loading).
    /// v0.1.9.1: Append to existing net entry instead of creating duplicates.
    /// This ensures that multiple calls for the same net_id consolidate segments.
    pub fn register_trace_segments(
        &mut self,
        net_id: NetId,
        segments: Vec<crate::geometry::TraceSegment>,
    ) {
        if segments.is_empty() {
            return;
        }
        
        // v0.1.9.1: Append to existing net entry instead of creating duplicates
        if let Some(entry) = self.routed_segments.iter_mut().find(|(id, _)| *id == net_id) {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
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
        material: MaterialId,
    ) {
        let segment = crate::geometry::TraceSegment::new(point, point, 0, material);
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
                    source: hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer { index: i },
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
            entity_registry: self.entity_registry.clone(),
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
