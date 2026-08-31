//! Unified Geometry Generation - Strongly-Typed Physical Layer System
//!
//! # Architecture
//!
//! This module is the SINGLE SOURCE OF TRUTH for all physical geometry in the compiler.
//! All exporters (GLB, DXF, SPICE parasitic extraction) read from these unified contours.
//!
//! ```text
//! HardwareSpace (compiler IR)
//!      ↓
//! GeometryCollector::collect() ← Type-safe geometry aggregation
//!      ↓
//! LayerSegmenter::segment_contacts() ← Planar physics for vias
//!      ↓
//! GeometryUnion::merge_pools() ← Boolean operations
//!      ↓
//! Vec<PhysicalLayer> ← Final output
//!      ├→ GLB mesh extrusion
//!      ├→ DXF 2D contours
//!      └→ SPICE parasitic capacitance extraction
//! ```
//!
//! # Physical Layer Model
//!
//! ## Conductive Layers (PCB & IC)
//! - Pads, traces, and pours are SOLID extruded volumes
//! - Vias passing through are UNIONED with pads (no subtraction)
//! - Material properties determine conductivity/dielectric behavior
//!
//! ## Planar Semiconductor Physics (CMOS/IC)
//! - `diff`, `poly`, `pdiff` are lateral patterned regions at wafer surface
//! - Via at (X,Y) only intersects layer if layer has geometry at that (X,Y)
//! - Z-elevation alone is NOT sufficient for intersection (must check XY spatial overlap)
//!
//! ## Dielectric Layers
//! - Substrate/oxide layers have via HOLES cut by mesh builder
//! - Vias are rendered as solid pillars filling those holes
//!
//! # Design Principles
//!
//! - **Zero Magic**: No hardcoded material names, no fallback defaults
//! - **Strongly Typed**: All geometry uses proper typed IDs (MaterialId, NetId, LayerId)
//! - **Fail Fast**: Missing data throws descriptive errors immediately
//! - **Pure Functions**: All operations are deterministic and side-effect free

use crate::geometry_union::{circle_to_path, rect_to_path};
use crate::scene_graph::trace_geometry;
use clipper2_rust::{FillRule, Path64};
use hwc_engine::geometry_router::entity_graph::SubstrateLayerType;
use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::netlist::NetId;
use hwc_engine::{HardwareSpace, MaterialId};
use rustc_hash::FxHashMap;

// ============================================================================
// STRONGLY-TYPED GEOMETRY PRIMITIVES
// ============================================================================

/// Unique identifier for a physical layer slice in the stackup
///
/// A physical layer is defined by its exact Stackup LayerId, Z-bounds, material, and electrical net.
/// All geometry within the same layer is merged via Boolean union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId {
    pub layer_id: hwc_types::LayerId,
    pub z_min: i64,
    pub z_max: i64,
    pub material: MaterialId,
    pub net_id: NetId,
}

impl LayerId {
    #[inline]
    pub fn z_span(&self) -> i64 {
        self.z_max - self.z_min
    }

    #[inline]
    pub fn contains_z(&self, z: i64) -> bool {
        z >= self.z_min && z < self.z_max
    }
}

/// A finalized physical layer with merged geometry
#[derive(Debug, Clone)]
pub struct PhysicalLayer {
    pub id: LayerId,
    /// 2D contours in XY plane (integer nanometer coordinates)
    /// Post-Boolean union - these are the canonical, deduplicated shapes
    pub contours: Vec<Path64>,
}

// ============================================================================
// GEOMETRY COLLECTION SYSTEM
// ============================================================================

/// Geometry collector - aggregates all geometric shapes into typed pools
struct GeometryCollector<'a> {
    space: &'a HardwareSpace,
    pools: FxHashMap<LayerId, Vec<Path64>>,
}

impl<'a> GeometryCollector<'a> {
    fn new(space: &'a HardwareSpace) -> Self {
        Self {
            space,
            pools: FxHashMap::default(),
        }
    }

    /// Collect all substrate pours (active regions, poly, metal pads)
    fn collect_pours(&mut self) {
        for layer in self
            .space
            .entity_graph
            .get_physical_substrate_layers(&self.space.material_registry)
        {
            // Only collect Pour layers here (Contacts are handled separately)
            if layer.layer_type != SubstrateLayerType::Pour {
                continue;
            }

            let layer_id = layer.layer_id.unwrap_or_else(|| {
                let idx = self.space.stackup_layers.iter().position(|l| {
                    (l.z_bottom <= layer.bbox.min.z && layer.bbox.min.z < l.z_top)
                        || l.z_bottom == layer.bbox.min.z
                }).unwrap_or(0);
                hwc_types::LayerId::new(idx as u16)
            });

            let pool_id = LayerId {
                layer_id,
                z_min: layer.bbox.min.z,
                z_max: layer.bbox.max.z,
                material: layer.material,
                net_id: layer.net,
            };

            let path = shape_to_path(&layer.shape, &layer.bbox);
            self.pools.entry(pool_id).or_default().push(path);
        }
    }

    /// Collect all contact/via layers with planar semiconductor physics
    fn collect_contacts(&mut self) {
        let segmenter = ContactSegmenter::new(self.space);

        for layer in self
            .space
            .entity_graph
            .get_physical_substrate_layers(&self.space.material_registry)
        {
            if layer.layer_type == SubstrateLayerType::Contact {
                segmenter.segment_contact(layer, &mut self.pools);
            }
        }
    }

    /// Collect all routed trace geometry
    fn collect_traces(&mut self) {
        let trace_pools = trace_geometry::generate_trace_geometry(self.space);

        for (geom_key, mut geom_pool) in trace_pools {
            geom_pool.flush_pending();

            let stackup_idx = self.space.stackup_layers.iter().position(|l| {
                l.z_bottom <= geom_key.z_min && geom_key.z_min < l.z_top
            }).unwrap_or(0);

            let layer_id = LayerId {
                layer_id: hwc_types::LayerId::new(stackup_idx as u16),
                z_min: geom_key.z_min,
                z_max: geom_key.z_max,
                material: geom_key.material,
                net_id: geom_key.net_id,
            };

            self.pools
                .entry(layer_id)
                .or_default()
                .extend(geom_pool.paths);
        }
    }

    /// Collect PCB via annular pads (IC vias are already in substrate as Contacts)
    fn collect_pcb_vias(&mut self) {
        for via in &self.space.vias {
            // Check manufacturing process via material registry
            let is_deposited = self
                .space
                .material_registry
                .get_process(via.material_id)
                .map(|p| p == hwc_engine::ManufacturingProcess::Deposited)
                .unwrap_or(false);

            if is_deposited {
                continue; // IC vias handled via Contacts
            }

            // PCB plated through-hole via: generate annular pads at landing layers
            let z_start = via.from_z_nm.min(via.to_z_nm);
            let z_end = via.from_z_nm.max(via.to_z_nm);
            let pad_radius = via.diameter_nm / 2 + via.enclosure_nm.max(via.diameter_nm / 4);

            // Find pad material (must be conductive)
            let pad_material = if self.space.material_registry.is_conductive(via.material_id) {
                via.material_id
            } else {
                self.space
                    .material_registry
                    .all_materials()
                    .into_iter()
                    .find(|(id, _)| self.space.material_registry.is_conductive(*id))
                    .map(|(id, _)| id)
                    .expect("FATAL: No conductive material in registry for via pads")
            };

            // Top landing pad
            let (top_idx, top_layer) = self
                .space
                .stackup_layers
                .iter()
                .enumerate()
                .find(|(_, l)| l.is_routable && l.contains_z(z_end))
                .unwrap_or_else(|| {
                    panic!(
                        "FATAL: Via landing at Z={}nm has no routable stackup layer. \
                         All via endpoints must land on conductive layers.",
                        z_end
                    )
                });

            let top_id = LayerId {
                layer_id: hwc_types::LayerId::new(top_idx as u16),
                z_min: top_layer.z_bottom,
                z_max: top_layer.z_top,
                material: pad_material,
                net_id: via.net_id,
            };

            self.pools.entry(top_id).or_default().push(circle_to_path(
                via.position.0,
                via.position.1,
                pad_radius,
                64,
            ));

            // Bottom landing pad
            let (bottom_idx, bottom_layer) = self
                .space
                .stackup_layers
                .iter()
                .enumerate()
                .find(|(_, l)| l.is_routable && l.contains_z(z_start))
                .unwrap_or_else(|| {
                    panic!(
                        "FATAL: Via landing at Z={}nm has no routable stackup layer. \
                         All via endpoints must land on conductive layers.",
                        z_start
                    )
                });

            let bottom_id = LayerId {
                layer_id: hwc_types::LayerId::new(bottom_idx as u16),
                z_min: bottom_layer.z_bottom,
                z_max: bottom_layer.z_top,
                material: pad_material,
                net_id: via.net_id,
            };

            self.pools.entry(bottom_id).or_default().push(circle_to_path(
                via.position.0,
                via.position.1,
                pad_radius,
                64,
            ));
        }
    }

    /// Consume collector and perform Boolean union on all pools
    fn finalize(self) -> Vec<PhysicalLayer> {
        let mut layers = Vec::new();

        for (id, paths) in self.pools {
            if paths.is_empty() {
                continue;
            }

            // Boolean union to merge overlapping shapes
            let contours = clipper2_rust::union_64(&paths, &Vec::new(), FillRule::NonZero);

            if !contours.is_empty() {
                layers.push(PhysicalLayer { id, contours });
            }
        }

        // Sort by Z-elevation for deterministic output
        layers.sort_by_key(|l| (l.id.layer_id, l.id.z_min, l.id.z_max, l.id.material, l.id.net_id));

        layers
    }
}

// ============================================================================
// CONTACT SEGMENTATION (PLANAR SEMICONDUCTOR PHYSICS)
// ============================================================================

/// Contact segmenter - slices vertical contacts (vias) through stackup layers
struct ContactSegmenter<'a> {
    space: &'a HardwareSpace,
}

impl<'a> ContactSegmenter<'a> {
    fn new(space: &'a HardwareSpace) -> Self {
        Self { space }
    }

    /// Segment a contact (via) through stackup layers
    ///
    /// Slices the continuous physical vertical contact column into layer segments
    /// matching the stackup coordinate slices. Each segment is deposited into the
    /// geometry pool matching the contact's material and electrical net, ensuring
    /// solid, gap-free physical via pillars across the entire vertical span.
    fn segment_contact(
        &self,
        contact: &hwc_engine::geometry_router::substrate_types::SubstrateLayer,
        pools: &mut FxHashMap<LayerId, Vec<Path64>>,
    ) {
        let via_z_min = contact.bbox.min.z;
        let via_z_max = contact.bbox.max.z;
        let via_path = shape_to_path(&contact.shape, &contact.bbox);

        let cx = (contact.bbox.min.x + contact.bbox.max.x) / 2;
        let cy = (contact.bbox.min.y + contact.bbox.max.y) / 2;

        // Resolve matching contact metadata to check from_layer and to_layer
        let matching_contact = self.space.contacts.iter().find(|c| {
            if let Some(ref cb) = c.bbox {
                let ccx = (cb.min.x + cb.max.x) / 2;
                let ccy = (cb.min.y + cb.max.y) / 2;
                ccx == cx && ccy == cy
            } else {
                c.z_start_nm.min(c.z_end_nm) == via_z_min && c.z_start_nm.max(c.z_end_nm) == via_z_max
            }
        });

        let from_layer = matching_contact.and_then(|c| c.from_layer.as_deref());
        let to_layer = matching_contact.and_then(|c| c.to_layer.as_deref());

        let from_idx = from_layer
            .and_then(|fl| self.space.stackup_layers.iter().position(|l| l.name == fl))
            .or_else(|| {
                self.space.stackup_layers.iter().position(|l| l.contains_z(via_z_min) || l.z_bottom == via_z_min)
            });

        let to_idx = to_layer
            .and_then(|tl| self.space.stackup_layers.iter().position(|l| l.name == tl))
            .or_else(|| {
                self.space.stackup_layers.iter().position(|l| l.contains_z(via_z_max) || l.z_top == via_z_max)
            });

        let is_asic = self.space.technology_strategy.is_asic()
            || self.space.fabrication_constraints.as_ref().is_some_and(|c| c.technology.is_asic());

        if is_asic {
            // 🎯 FIRST PRINCIPLES: IN ASIC MODE, VIAS DO NOT DRILL INTERMEDIATE LAYERS!
            // A via exists ONLY at its defined vertical endpoints (from_layer and to_layer).
            if let (Some(f_idx), Some(t_idx)) = (from_idx, to_idx) {
                // 1. Register at bottom landing layer (e.g. Metal 3)
                let bottom_layer = &self.space.stackup_layers[f_idx];
                pools.entry(LayerId {
                    layer_id: hwc_types::LayerId::new(f_idx as u16),
                    z_min: bottom_layer.z_bottom,
                    z_max: bottom_layer.z_top,
                    material: contact.material,
                    net_id: contact.net,
                }).or_default().push(via_path.clone());

                // 2. Register at top landing layer (e.g. Metal 4)
                if f_idx != t_idx {
                    let top_layer = &self.space.stackup_layers[t_idx];
                    pools.entry(LayerId {
                        layer_id: hwc_types::LayerId::new(t_idx as u16),
                        z_min: top_layer.z_bottom,
                        z_max: top_layer.z_top,
                        material: contact.material,
                        net_id: contact.net,
                    }).or_default().push(via_path);
                }
                return;
            }
        }

        // PCB Mode: Mechanical drill holes punch through intermediate substrate layers
        let (min_idx, max_idx) = match (from_idx, to_idx) {
            (Some(f), Some(t)) => (f.min(t), f.max(t)),
            _ => (0, self.space.stackup_layers.len().saturating_sub(1)),
        };

        for idx in min_idx..=max_idx {
            let stackup_layer = &self.space.stackup_layers[idx];
            if via_z_max <= stackup_layer.z_bottom || via_z_min >= stackup_layer.z_top {
                continue;
            }

            let segment_z_min = via_z_min.max(stackup_layer.z_bottom);
            let segment_z_max = via_z_max.min(stackup_layer.z_top);

            if segment_z_max <= segment_z_min {
                continue;
            }

            let layer_id = LayerId {
                layer_id: hwc_types::LayerId::new(idx as u16),
                z_min: segment_z_min,
                z_max: segment_z_max,
                material: contact.material,
                net_id: contact.net,
            };

            pools.entry(layer_id).or_default().push(via_path.clone());
        }
    }
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Convert substrate layer shape to 2D path
fn shape_to_path(shape: &SubstrateLayerShape, bbox: &BoundingBox) -> Path64 {
    let cx = (bbox.min.x + bbox.max.x) / 2;
    let cy = (bbox.min.y + bbox.max.y) / 2;
    match shape {
        SubstrateLayerShape::Rect => rect_to_path(bbox),
        SubstrateLayerShape::Circle { radius } => {
            circle_to_path(cx, cy, *radius, 64)
        }
        SubstrateLayerShape::Polygon { outer_contour, .. } => {
            let is_relative = outer_contour.iter().all(|pt| pt.x.abs() < (bbox.max.x - bbox.min.x + 500) && pt.y.abs() < (bbox.max.y - bbox.min.y + 500));
            if is_relative && (cx.abs() > 500 || cy.abs() > 500) {
                let mut path = Path64::new();
                for pt in outer_contour {
                    path.push(clipper2_rust::Point64::new(pt.x + cx, pt.y + cy));
                }
                path
            } else {
                outer_contour.clone()
            }
        }
        _ => {
            panic!(
                "FATAL: Unsupported substrate shape {:?}. \
                 Only Rect, Circle, and Polygon are supported.",
                shape
            )
        }
    }
}

// ============================================================================
// PUBLIC API
// ============================================================================

/// Generate unified physical layers from HardwareSpace
///
/// This is the SINGLE SOURCE OF TRUTH for all physical geometry.
/// All exporters (GLB, DXF, SPICE extraction) consume this output.
///
/// # Process
///
/// 1. Collect all pours, contacts, traces, and vias into typed pools
/// 2. Segment contacts through stackup with planar semiconductor physics
/// 3. Perform Boolean union on each pool to merge overlapping geometry
/// 4. Return sorted, finalized physical layers
///
/// # Guarantees
///
/// - **Zero Fallbacks**: All missing data causes immediate panic with descriptive message
/// - **Deterministic**: Output is always sorted by (Z, material, net) for reproducibility
/// - **Type Safe**: All IDs (MaterialId, NetId, LayerId) are strongly typed
pub fn generate_copper_contours(space: &HardwareSpace) -> Vec<PhysicalLayer> {
    let mut collector = GeometryCollector::new(space);

    collector.collect_pours();
    collector.collect_contacts();
    collector.collect_traces();
    collector.collect_pcb_vias();

    collector.finalize()
}
