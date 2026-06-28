mod enums;
mod metadata;
mod primitives;
mod traces;

pub use enums::*;
pub use metadata::*;
pub use primitives::*;
pub use traces::*;

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::EntityGraph;
use crate::netlist::{NetId, NetlistArena};
use crate::material::{MaterialId, MaterialRegistry};

use compact_str::CompactString;
use rustc_hash::FxHashMap;

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

    /// **v0.1.8: Coordinate snapping resolution in nanometers.**
    pub resolution_nm: i64,
}

impl HardwareSpace {
    /// Create a new hardware space with entity graph and netlist.
    pub fn new(
        name: CompactString,
        dimensions: Dimensions,
        substrate_material_id: MaterialId,
        material_registry: MaterialRegistry,
        view: SpaceView,
        resolution_nm: i64,
    ) -> Self {
        let entity_graph = EntityGraph::new();
        let netlist = NetlistArena::new();

        Self {
            name,
            dimensions,
            substrate_material_id,
            material_registry,
            view,
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
            resolution_nm,
        }
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

    /// **v0.1.7: Add an analytic route (GOD-TIER NATIVE API)**
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
            .drill_hole(hole_bbox, diameter_nm, drill_net.raw());
    }

    /// **v0.1.7: Realize analytic routes into substrate layers (LAZY REALIZATION)**
    pub fn realize_analytic_routes(&mut self) {
        let _start = std::time::Instant::now();

        let mut groups: FxHashMap<(i64, i64, MaterialId, u32), Vec<BoundingBox>> =
            FxHashMap::default();

        for route in &self.analytic_routes {
            let half_w = route.width_nm / 2;

            for seg in &route.segments {
                let half_t = route.thickness_nm / 2;
                let z_min = seg.start.z.min(seg.end.z) - half_t;
                let z_max = seg.start.z.max(seg.end.z) + half_t;

                let x_min = seg.start.x.min(seg.end.x) - half_w;
                let x_max = seg.start.x.max(seg.end.x) + half_w;
                let y_min = seg.start.y.min(seg.end.y) - half_w;
                let y_max = seg.start.y.max(seg.end.y) + half_w;

                if x_max - x_min < 100 || y_max - y_min < 100 || z_max - z_min < 100 {
                    continue;
                }

                let bbox = BoundingBox::new(
                    Point3D::new(x_min, y_min, z_min),
                    Point3D::new(x_max, y_max, z_max),
                );

                let key = (z_min, z_max, route.material, route.net_id.0);
                groups.entry(key).or_default().push(bbox);
            }
        }

        for ((z_min, z_max, material, net), bboxes) in groups {
            let group_bbox = BoundingBox::new(
                Point3D::new(
                    bboxes.iter().map(|b| b.min.x).min().unwrap_or(0),
                    bboxes.iter().map(|b| b.min.y).min().unwrap_or(0),
                    z_min,
                ),
                Point3D::new(
                    bboxes.iter().map(|b| b.max.x).max().unwrap_or(0),
                    bboxes.iter().map(|b| b.max.y).max().unwrap_or(0),
                    z_max,
                ),
            );

            let mut layer = crate::geometry_router::substrate_types::SubstrateLayer::new(
                material,
                net,
                group_bbox,
                crate::geometry_router::substrate_types::SubstrateLayerType::Pour,
            );
            for bbox in bboxes {
                layer.append_region(bbox);
            }
            self.entity_graph.get_substrate_layers_mut().push(layer);
        }

        let vias = self.vias.clone();
        for via in vias {
            let z_start = via.from_z_nm.min(via.to_z_nm);
            let z_end = via.from_z_nm.max(via.to_z_nm);
            let hole_bbox = BoundingBox::new(
                Point3D::new(
                    via.position.0 - via.diameter_nm / 2,
                    via.position.1 - via.diameter_nm / 2,
                    z_start,
                ),
                Point3D::new(
                    via.position.0 + via.diameter_nm / 2,
                    via.position.1 + via.diameter_nm / 2,
                    z_end,
                ),
            );

            let pad_diameter = via.diameter_nm
                + 2 * if via.annular_ring_nm > 0 {
                    via.annular_ring_nm
                } else {
                    via.diameter_nm / 2
                };
            self.entity_graph.drill_via_hole(
                hole_bbox,
                via.diameter_nm,
                via.net_id.raw(),
                0,
                false,
                pad_diameter,
                75_000,
            );
        }
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

                    pins.iter().find_map(|&pin_id| {
                        let pin_data = self.netlist.get_pin(pin_id)?;
                        if pin_data.name == binding.terminal {
                            let net_id = pin_data.connected_net?;
                            let net_data = self.netlist.get_net(net_id)?;
                            Some((net_data.name.to_string(), net_id))
                        } else {
                            None
                        }
                    })
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
                        layer.net = net_id.raw();
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
                                layer.net = net_id.raw();
                            }
                        }
                    }
                }
            }
        }
    }
}
