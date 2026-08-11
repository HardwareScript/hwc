mod enums;
mod metadata;
mod primitives;
mod traces;

pub use enums::*;
pub use metadata::*;
pub use primitives::*;
pub use traces::*;

use crate::geometry::BoundingBox;
use crate::geometry_router::EntityGraph;
use crate::material::{MaterialId, MaterialRegistry};
use crate::netlist::{NetId, NetlistArena};

use compact_str::CompactString;
use hwc_types::Technology;
use rustc_hash::FxHashMap;

/// Parameters for creating a new HardwareSpace
pub struct HardwareSpaceParams {
    pub name: CompactString,
    pub dimensions: Dimensions,
    pub substrate_material_id: MaterialId,
    pub material_registry: MaterialRegistry,
    pub view: SpaceView,
    /// Manufacturing grid in nanometers, derived from the PDK profile
    /// (`manufacturing.track_pitch`, falling back to
    /// `manufacturing.min_feature_size`). v0.2.1: never user-declared.
    pub manufacturing_grid_nm: i64,
    pub technology_strategy: Technology,
}

/// **v0.2.0: Stackup layer information (single source of truth)**
///
/// Minimal stackup metadata embedded in HardwareSpace so export and validation
/// code can resolve layer Z-coordinates without needing the full StackupManager.
#[derive(Debug, Clone)]
pub struct StackupLayer {
    /// Layer name (e.g., "metal1", "poly", "active")
    pub name: CompactString,
    /// Physical bottom Z in nanometers (from Z=0 reference)
    pub z_bottom: i64,
    /// Physical top Z in nanometers
    pub z_top: i64,
    /// Layer thickness in nanometers
    pub thickness: i64,
    /// Material name for this layer
    pub material_name: CompactString,
    /// Whether this layer is routable (conductive)
    pub is_routable: bool,
    /// **v0.2.1: Whether this layer is a zero-thickness mask.**
    ///
    /// Mask layers are 2D fabrication instructions (chemical process masks,
    /// passivation openings). They anchor to a Z-plane but have 0nm thickness,
    /// never participate in routing, and are excluded from physical collision
    /// and 3D mesh generation.
    pub is_mask: bool,
}

impl StackupLayer {
    pub fn new(
        name: CompactString,
        z_bottom: i64,
        z_top: i64,
        thickness: i64,
        material_name: CompactString,
        is_routable: bool,
        is_mask: bool,
    ) -> Self {
        Self {
            name,
            z_bottom,
            z_top,
            thickness,
            material_name,
            is_routable,
            is_mask,
        }
    }

    /// Get the centerline Z coordinate of this layer
    pub fn centerline_z(&self) -> i64 {
        (self.z_bottom + self.z_top) / 2
    }

    /// Check if a Z coordinate falls within this layer's physical bounds
    pub fn contains_z(&self, z: i64) -> bool {
        z >= self.z_bottom && z <= self.z_top
    }
}

/// Complete hardware space with entity graph and connectivity.
///
/// This structure combines:
/// - Physical dimensions and coordinate snapping
/// - Entity graph for substrate, pin, and component storage
/// - Component and net connectivity (ECS-style arena)
/// - Routing results (vias for drill file generation)
/// - Material registry for dynamic material support
/// - Pour metadata for BOM and netlist generation
/// - Net classifications for physics validation (v0.1.6)
/// - **Bounding box tracker for component placement (Sprint 3.10)**
/// - **Analytic route overlay for native geometry (v0.1.7 - GOD-TIER)**
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PadShape {
    Rect,
    Rectangle, // Alias for Rect
    Circle,
    Hexagon,
    Polygon,
    Obround,
    RoundedRect,
}

pub struct HardwareSpace {
    pub name: CompactString,
    pub dimensions: Dimensions,
    pub substrate_material_id: MaterialId,
    pub view: SpaceView,

    /// Material registry for dynamic material support
    pub material_registry: MaterialRegistry,

    /// Entity graph — single source of truth for substrate, pin, and component storage.
    pub entity_graph: EntityGraph,

    /// Netlist arena for component/pin/net connectivity
    pub netlist: NetlistArena,

    /// Vias placed during routing (for drill file export)
    pub vias: Vec<crate::geometry_router::Via>,

    /// Pour metadata for BOM and netlist generation
    pub pours: Vec<PourMetadata>,

    /// Contact metadata for connectivity checking
    pub contacts: Vec<ContactMetadata>,

    /// Net classifications for physics validation (v0.1.6)
    pub net_classifications: FxHashMap<CompactString, NetClassification>,

    /// Substrate bounding box for overlap validation (v0.1.6 GAP2)
    pub substrate_bbox: Option<crate::geometry::BoundingBox>,

    /// **Sprint 3.10: Component bounding box tracker (NATIVE ARCHITECTURE)**
    pub component_bboxes: FxHashMap<CompactString, crate::geometry::BoundingBox>,

    /// **v0.1.7: ANALYTIC ROUTE OVERLAY (GOD-TIER NATIVE ARCHITECTURE)**
    pub analytic_routes: Vec<AnalyticTrace>,

    /// **v0.1.6: Fabrication Constraints (DRC Integration)**
    pub fabrication_constraints: Option<hwc_materials::ConstraintSet>,

    /// **v0.1.7: Keep-Out Zones for DRC and routing (NATIVE)**
    pub keep_out_zones: Vec<KeepOutZone>,

    /// **v0.2.1: Manufacturing grid in nanometers.**
    ///
    /// Derived from the PDK profile (`manufacturing.track_pitch`, falling back
    /// to `manufacturing.min_feature_size`). Replaces the purged user-facing
    /// `resolution:` declaration, which created a dual-authority conflict with
    /// the profile. Used for coordinate snapping, router track pitch, and DRC.
    pub manufacturing_grid_nm: i64,

    /// **v0.2.0: Stackup layer metadata (single source of truth)**
    /// Ordered list of layers from bottom to top with physical Z-coordinates.
    /// Populated during compilation from the StackupManager.
    pub stackup_layers: Vec<StackupLayer>,

    /// **v0.2.0: Technology strategy determined from PDK profile**
    /// Set once during compilation and used consistently throughout all subsystems.
    /// No scattered conditionals - this is the single source of truth.
    pub technology_strategy: Technology,

    /// **v0.2.0: Hierarchical Routing Database (PROPER ARCHITECTURE)**
    /// Maintains clear separation between child-instance routes and parent-level
    /// interconnects. This is the single source of truth for routing connectivity
    /// validation in hierarchical designs.
    pub routing_database: crate::routing_database::HierarchicalRoutingDatabase,

    /// **v0.2.0: Database of all routing connection points (PROPER ARCHITECTURE)**
    /// Maps every entity to its exact layer connections with Z elevations.
    /// Populated during placement, queried during routing.
    pub layer_connection_db: crate::layer_connection_database::LayerConnectionDatabase,

    /// **v0.2.0: Database of routing layer Z elevations (PROPER ARCHITECTURE)**
    /// Single source of truth for which Z coordinate to route on each layer.
    /// Built from stackup + material registry.
    pub routing_layer_db: crate::routing_layer_database::RoutingLayerDatabase,

    /// **v0.2.0: Database of via-to-layer mappings (PROPER ARCHITECTURE)**
    /// Maps material pairs to via connection specs. Built from bridge rules + stackup.
    pub via_layer_mapping_db: crate::via_layer_mapping_database::ViaLayerMappingDatabase,

    /// **v0.2.0: Database of explicit via/contact instances (PROPER ARCHITECTURE)**
    /// Tracks all user-defined vias to prevent duplicate auto-insertion. Populated during placement.
    pub via_instance_db: crate::via_instance_database::ViaInstanceDatabase,

    /// **v0.2.1: Device instances registry (Native Device Support - PROPER ARCHITECTURE)**
    /// Single source of truth for all device instances extracted from pour bindings.
    /// Populated during compilation from device definitions and pour bindings.
    /// Used by SPICE exporter, BOM generator, and other export formats.
    /// This eliminates guessing and provides clean separation between compilation and export.
    pub device_instances: Vec<crate::space::DeviceInstance>,
}

impl HardwareSpace {
    /// Create a new hardware space with entity graph and netlist.
    pub fn new(params: HardwareSpaceParams) -> Self {
        let entity_graph = EntityGraph::new();
        let netlist = NetlistArena::new();

        Self {
            name: params.name,
            dimensions: params.dimensions,
            substrate_material_id: params.substrate_material_id,
            material_registry: params.material_registry,
            view: params.view,
            entity_graph,
            netlist,
            vias: Vec::new(),
            pours: Vec::new(),
            contacts: Vec::new(),
            net_classifications: FxHashMap::default(),
            substrate_bbox: None,
            component_bboxes: FxHashMap::default(),
            analytic_routes: Vec::new(),
            fabrication_constraints: None,
            keep_out_zones: Vec::new(),
            manufacturing_grid_nm: params.manufacturing_grid_nm,
            stackup_layers: Vec::new(),
            technology_strategy: params.technology_strategy,
            routing_database: crate::routing_database::HierarchicalRoutingDatabase::new(),
            layer_connection_db: crate::layer_connection_database::LayerConnectionDatabase::new(),
            routing_layer_db: crate::routing_layer_database::RoutingLayerDatabase::default(),
            via_layer_mapping_db:
                crate::via_layer_mapping_database::ViaLayerMappingDatabase::default(),
            via_instance_db: crate::via_instance_database::ViaInstanceDatabase::new(),
            device_instances: Vec::new(),
        }
    }

    /// **v0.2.0: Find the stackup layer containing a given Z coordinate**
    /// Returns None if Z is outside all layer bounds (air gap or out of bounds).
    pub fn find_layer_at_z(&self, z: i64) -> Option<&StackupLayer> {
        let count = self.stackup_layers.len();
        for (idx, layer) in self.stackup_layers.iter().enumerate() {
            let is_top = idx == count - 1;
            let contains = if is_top {
                z >= layer.z_bottom && z <= layer.z_top
            } else {
                z >= layer.z_bottom && z < layer.z_top
            };
            if contains {
                return Some(layer);
            }
        }
        None
    }

    /// **v0.2.0: Get layer by name**
    pub fn get_layer_by_name(&self, name: &str) -> Option<&StackupLayer> {
        self.stackup_layers
            .iter()
            .find(|layer| layer.name.as_str() == name)
    }

    /// Register a component bbox and its metadata in the entity graph.
    pub fn register_component_bbox(
        &mut self,
        name: CompactString,
        bbox: crate::geometry::BoundingBox,
        material_id: MaterialId,
        component_type: CompactString,
        blocked_z_ranges: smallvec::SmallVec<[(i64, i64); 2]>,
    ) {
        self.component_bboxes.insert(name.clone(), bbox);
        self.entity_graph.add_component_metadata(
            bbox,
            material_id,
            name,
            component_type,
            blocked_z_ranges,
        );
    }

    pub fn set_net_classification(
        &mut self,
        net_name: CompactString,
        classification: NetClassification,
    ) {
        self.net_classifications.insert(net_name, classification);
    }

    pub fn get_net_classification(&self, net_name: &str) -> NetClassification {
        self.net_classifications
            .get(net_name)
            .copied()
            .unwrap_or(NetClassification::Unclassified)
    }

    pub fn add_vias(&mut self, vias: Vec<crate::geometry_router::Via>) {
        self.vias.extend(vias);
    }

    /// **v0.1.7: Register a Keep-Out Zone (KOZ)**
    pub fn register_keep_out_zone(&mut self, koz: KeepOutZone) {
        self.keep_out_zones.push(koz);
    }

    /// **Sprint 3.10: Get all component bounding boxes (for SDF)**
    pub fn iter_component_bboxes(
        &self,
    ) -> impl Iterator<Item = (&CompactString, &crate::geometry::BoundingBox)> {
        self.component_bboxes.iter()
    }

    /// **v0.2.0: Get analytic routes (read-only view)**
    ///
    /// Routes are derived from the routing database. To add routes,
    /// use `routing_database` methods directly.
    pub fn get_analytic_routes(&self) -> &[AnalyticTrace] {
        &self.analytic_routes
    }

    /// **v0.2.0: Rebuild analytic_routes from routing database**
    ///
    /// Called after routing operations complete to sync the derived view.
    pub fn sync_analytic_routes_from_database(&mut self) {
        eprintln!(
            "[SYNC] sync_analytic_routes_from_database() called for space '{}'",
            self.name
        );
        eprintln!(
            "[SYNC]   Before sync: analytic_routes.len() = {}",
            self.analytic_routes.len()
        );

        self.analytic_routes = self
            .routing_database
            .build_analytic_routes(&self.netlist, &self.stackup_layers);

        eprintln!(
            "[SYNC]   After sync: analytic_routes.len() = {}",
            self.analytic_routes.len()
        );
        eprintln!("[SYNC]   Routing database stats:");
        eprintln!(
            "[SYNC]     - parent_interconnects: {}",
            self.routing_database.get_parent_interconnects().len()
        );
    }

    /// **v0.2.0: Add an analytic route (GOD-TIER NATIVE API)**
    #[deprecated(
        since = "0.2.0",
        note = "Use routing_database methods directly. This method will be removed in v0.3.0."
    )]
    pub fn add_analytic_route(&mut self, route: AnalyticTrace) {
        self.analytic_routes.push(route);
    }

    /// **v0.1.7: Drill a hole through all substrate layers (Limitation 7)**
    pub fn drill_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: Option<i64>,
        drill_net: NetId,
    ) {
        self.entity_graph
            .drill_hole(hole_bbox, diameter_nm, drill_net);
    }

    /// **v0.1.7: Synchronize net names from pins to bound pours**
    pub fn synchronize_nets(&mut self) {
        let mut updates = Vec::new();

        for (pour_idx, pour) in self.pours.iter().enumerate() {
            if let Some(binding) = &pour.device_binding {
                let resolved_opt = (|| {
                    let comp_id = self
                        .netlist
                        .get_component_by_name(binding.device_name.as_str())?;
                    let pins = self.netlist.get_component_pins(comp_id);

                    // v0.2.2: Check all terminals in the binding
                    for terminal in &binding.terminals {
                        if let Some(result) = pins.iter().find_map(|&pin_id| {
                            let pin_data = self.netlist.get_pin(pin_id)?;
                            if pin_data.name == *terminal {
                                let net_id = pin_data.connected_net?;
                                let net_data = self.netlist.get_net(net_id)?;
                                Some((net_data.name.to_string(), net_id))
                            } else {
                                None
                            }
                        }) {
                            return Some(result);
                        }
                    }
                    None
                })();

                if let Some((net_name, net_id)) = resolved_opt {
                    updates.push((
                        pour_idx,
                        net_name,
                        net_id,
                        pour.bbox,
                        pour.material_name.clone(),
                    ));
                }
            }
        }

        for (pour_idx, net_name, net_id, bbox, material_name) in updates {
            self.pours[pour_idx].net = Some(net_name.into());

            if let Some(pour_bbox) = bbox {
                let material_id = self.material_registry.get_id(&material_name).unwrap_or_else(|| {
                    panic!(
                        "Internal error: material '{}' should have been registered during pour placement",
                        material_name
                    )
                });
                for layer in self.entity_graph.get_substrate_layers_mut() {
                    if layer.layer_type
                        == crate::geometry_router::substrate_types::SubstrateLayerType::Pour
                        && layer.material == material_id
                        && layer.bbox == pour_bbox
                    {
                        layer.net = net_id;
                    }
                }
            }
        }

        for contact in &self.contacts {
            if let Some(net_name) = &contact.net {
                if let Some(net_id) = self.netlist.get_net_by_name(net_name.as_str()) {
                    if let Some(contact_bbox) = contact.bbox {
                        let material_id = self.material_registry.get_id(&contact.material_name).unwrap_or_else(|| {
                            panic!(
                                "Internal error: material '{}' should have been registered during contact placement",
                                contact.material_name
                            )
                        });
                        for layer in self.entity_graph.get_substrate_layers_mut() {
                            if layer.material == material_id && layer.bbox == contact_bbox {
                                layer.net = net_id;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// **v0.2.0: MiterContext implementation for HardwareSpace**
///
/// Provides via/contact location queries to the miter engine, allowing it to
/// preserve connections to via landing pads by skipping miter on terminal segments.
impl crate::geometry_router::miter_pass::MiterContext for HardwareSpace {
    fn is_via_endpoint(
        &self,
        point: &crate::geometry::Point3D,
        net_id: Option<NetId>,
        tolerance_nm: i64,
    ) -> bool {
        // Query contact metadata to check if this point is a via center
        for contact in &self.contacts {
            // Skip if net doesn't match (when net filtering is requested)
            if let Some(query_net) = net_id {
                if let Some(contact_net_name) = &contact.net {
                    if let Some(contact_net_id) =
                        self.netlist.get_net_by_name(contact_net_name.as_str())
                    {
                        if contact_net_id != query_net {
                            continue; // Wrong net, skip this contact
                        }
                    }
                }
            }

            // Check if point is within tolerance of contact bbox center
            if let Some(bbox) = contact.bbox {
                let center_x = (bbox.min.x + bbox.max.x) / 2;
                let center_y = (bbox.min.y + bbox.max.y) / 2;
                let center_z = (bbox.min.z + bbox.max.z) / 2;

                let dx = (point.x - center_x).abs();
                let dy = (point.y - center_y).abs();
                let dz = (point.z - center_z).abs();

                // Check if point is within tolerance of contact center (XY plane)
                // Z can be different (routing layer vs via center) so check Z separately
                if dx <= tolerance_nm && dy <= tolerance_nm && dz <= bbox.max.z - bbox.min.z {
                    return true;
                }
            }
        }
        false
    }

    fn get_contact_bbox(
        &self,
        point: &crate::geometry::Point3D,
        tolerance_nm: i64,
    ) -> Option<BoundingBox> {
        for contact in &self.contacts {
            if let Some(bbox) = contact.bbox {
                let center_x = (bbox.min.x + bbox.max.x) / 2;
                let center_y = (bbox.min.y + bbox.max.y) / 2;

                let dx = (point.x - center_x).abs();
                let dy = (point.y - center_y).abs();

                if dx <= tolerance_nm && dy <= tolerance_nm {
                    return Some(bbox);
                }
            }
        }
        None
    }
}
