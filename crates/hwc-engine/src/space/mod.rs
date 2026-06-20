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
use crate::voxel::{MaterialId, MaterialRegistry};

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
#[derive(Debug)]
pub struct HardwareSpace {
    pub name: CompactString,
    pub dimensions: Dimensions,
    pub grid: GridCells,
    pub voxel_size: VoxelSize,
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
    pub resolution_nm: Option<i64>,
}

impl HardwareSpace {
    /// Create a new hardware space with entity graph and netlist.
    pub fn new(
        name: CompactString,
        dimensions: Dimensions,
        grid: GridCells,
        substrate_material_id: MaterialId,
        material_registry: MaterialRegistry,
        view: SpaceView,
    ) -> Self {
        let voxel_size = VoxelSize::from_dimensions(dimensions, grid);
        let entity_graph = EntityGraph::new();
        let netlist = NetlistArena::new();

        Self {
            name,
            dimensions,
            grid,
            voxel_size,
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
            resolution_nm: None,
        }
    }

    /// Derive grid cell counts from dimensions and voxel size.
    ///
    /// This replaces the removed `grid: GridCells` field.
    pub fn grid_cells(&self) -> GridCells {
        GridCells::new(
            ((self.dimensions.width_nm + self.voxel_size.x_nm - 1) / self.voxel_size.x_nm) as usize,
            ((self.dimensions.height_nm + self.voxel_size.y_nm - 1) / self.voxel_size.y_nm) as usize,
            ((self.dimensions.depth_nm + self.voxel_size.z_nm - 1) / self.voxel_size.z_nm) as usize,
        )
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

    /// Get net classification (v0.1.6)
    pub fn get_net_classification(&self, net_name: &str) -> NetClassification {
        if let Some(&classification) = self.net_classifications.get(net_name) {
            return classification;
        }
        NetClassification::Unclassified
    }

    /// Set net classification (v0.1.6)
    pub fn set_net_classification(
        &mut self,
        net_name: CompactString,
        classification: NetClassification,
    ) {
        self.net_classifications.insert(net_name, classification);
    }

    /// Add vias from routing results.
    pub fn add_vias(&mut self, vias: Vec<crate::geometry_router::Via>) {
        self.vias.extend(vias);
    }

    /// Convert voxel coordinates to physical position.
    pub fn voxel_to_position(&self, x: usize, y: usize, z: usize) -> Point3D {
        Point3D::new(
            x as i64 * self.voxel_size.x_nm,
            y as i64 * self.voxel_size.y_nm,
            z as i64 * self.voxel_size.z_nm,
        )
    }

    /// Convert physical position to voxel coordinates.
    pub fn position_to_voxel(&self, pos: Point3D) -> (usize, usize, usize) {
        (
            (pos.x / self.voxel_size.x_nm) as usize,
            (pos.y / self.voxel_size.y_nm) as usize,
            (pos.z / self.voxel_size.z_nm) as usize,
        )
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
        eprintln!(
            "[ANALYTIC ROUTES] Realizing {} routes into sparse substrate layers...",
            self.analytic_routes.len()
        );
        let start = std::time::Instant::now();

        let mut segment_count = 0;

        use rustc_hash::FxHashMap;
        let mut groups: FxHashMap<(i64, i64, MaterialId, u32), Vec<BoundingBox>> =
            FxHashMap::default();

        for route in &self.analytic_routes {
            let half_w = route.width_nm / 2;

            for seg in &route.segments {
                // v0.1.8: Fixed Z-axis 'Phantom Offset' bug.
                // The realization pipeline was calculating z_max as seg.z + thickness.
                // However, in a vector-first system, the anchor 'z' is the CENTER of 
                // the copper thickness. The realization must expand symmetrically.
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

                segment_count += 1;
            }
        }

        let mut layer_count = 0;
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

            let mut layer = crate::voxel_grid::SubstrateLayer::new(material, net, group_bbox, crate::geometry_router::entity_graph::SubstrateLayerType::Pour);
            for bbox in bboxes {
                layer.append_region(bbox);
            }
            self.entity_graph.get_substrate_layers_mut().push(layer);
            layer_count += 1;
        }

        let duration = start.elapsed();
        eprintln!(
            "[ANALYTIC ROUTES] Realization complete: {} segments → {} sparse layers ({}ms)",
            segment_count,
            layer_count,
            duration.as_millis()
        );

        // Post-realization drill pass for vias
        eprintln!(
            "[ANALYTIC ROUTES] Running post-realization drill pass for {} vias...",
            self.vias.len()
        );
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
                let material_id = self.material_registry.get_or_register(&material_name);
                for layer in self.entity_graph.get_substrate_layers_mut() {
                    if layer.layer_type == crate::geometry_router::entity_graph::SubstrateLayerType::Pour
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
                        let material_id = self
                            .material_registry
                            .get_or_register(&contact.material_name);
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
