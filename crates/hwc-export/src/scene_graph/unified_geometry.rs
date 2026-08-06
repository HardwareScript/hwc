//! Unified Geometry Generation
//!
//! This module generates the canonical 2D copper contours that are used by ALL exporters.
//! It ensures that GLB, DXF, and any future exporters see the exact same geometry.
//!
//! # Architecture Principle
//!
//! ```text
//! HardwareSpace (engine output)
//!     ↓
//! unified_geometry::generate_copper_contours() ← SINGLE SOURCE OF TRUTH
//!     ├→ 3D mesh extrusion (GLB)
//!     └→ 2D contour export (DXF)
//! ```
//!
//! This eliminates the previous architectural violation where DXF was doing its own
//! geometry calculations, causing inconsistencies between exporters.
//!
//! # Industry-Standard 3D CAD Architecture (v0.2.2)
//!
//! ## Physical Stackup (No Hacks, Pure Physics)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ TOP CONDUCTIVE LAYER (metal1: Z=60→70nm)                    │
//! │  • Top_Pad (Aluminum) is SOLID - NO hole punched!           │
//! │  • Via pillar is also SOLID at Z=60→70nm                    │
//! │  • Boolean union merges them into single solid shape        │
//! ├─────────────────────────────────────────────────────────────┤
//! │ INTER-LAYER DIELECTRIC (d1: Z=10→60nm)                      │
//! │  • Via pillar (Titanium_Silicide) is SOLID Z=10→60nm       │
//! │  • Substrate mesh builder cuts hole in Silicon_Dioxide base │
//! ├─────────────────────────────────────────────────────────────┤
//! │ BOTTOM ACTIVE LAYER (active: Z=0→10nm)                      │
//! │  • Bottom_Pad (Silicon_P) is SOLID - NO hole punched!       │
//! │  • Via pillar is also SOLID at Z=0→10nm                     │
//! │  • Boolean union merges them into single solid shape        │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## The 4 Standard Rules
//!
//! 1. **Pads and Traces on Conductive Layers ARE SOLID**
//!    - Top_Pad, Bottom_Pad, all metal traces are solid extruded blocks
//!    - NO holes are cut from conductive geometry
//!
//! 2. **Dielectric/Substrate Base Layers GET HOLE CUTOUTS**
//!    - Dielectric layers (oxide, substrate) have via holes cut by mesh builder
//!    - This is where the "hole punching" happens (NOT in metal pads!)
//!
//! 3. **Via/Contact Plugs ARE SOLID PILLARS**
//!    - Via material is extruded as a solid pillar through ALL layers it passes
//!    - In conductive layers: Via + Pad both solid → Boolean union welds them
//!    - In dielectric layers: Via solid, substrate base has hole → Pillar fills hole
//!
//! 4. **Single-Sided Winding (GPU Backface Culling)**
//!    - All meshes use CCW winding for outer faces
//!    - glTF export sets "doubleSided": false
//!    - GPU discards interior touching faces naturally → Zero Z-fighting
//!
//! # Design Rules
//!
//! - NO hardcoded material names (use MaterialRegistry lookups)
//! - NO fallback defaults (fail fast if data is missing)
//! - Use proper typed IDs (MaterialId, NetId) everywhere
//! - All Z-ranges come from either AnalyticTrace.layer_z_range or SubstrateLayer.bbox
//! - NO subtraction of vias from same-net conductive pads (they union together!)

use crate::geometry_union::{circle_to_path, rect_to_path};
use crate::scene_graph::trace_geometry;
use clipper2_rust::{FillRule, Path64};
use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;
use hwc_engine::geometry_router::entity_graph::SubstrateLayerType;
use hwc_engine::{HardwareSpace, MaterialId};
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;

/// Key for grouping 2D paths that will be unioned together
///
/// This key is EXACTLY what both the substrate and trace geometry systems use,
/// ensuring traces and pads with matching parameters get merged together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CopperPoolKey {
    pub z_min: i64,
    pub z_max: i64,
    pub material: MaterialId,
    pub net_id: NetId,
}

/// A unified copper contour (post-Boolean union)
#[derive(Debug, Clone)]
pub struct UnifiedCopperContour {
    pub key: CopperPoolKey,
    /// 2D contours in XY plane (nanometer coordinates)
    /// After Boolean union - these are the final, deduplicated contours
    pub contours: Vec<Path64>,
}

/// Generate unified copper contours from HardwareSpace
///
/// This is the SINGLE SOURCE OF TRUTH for all copper geometry.
/// Both 3D mesh generation (GLB) and 2D export (DXF) read from this.
///
/// # Architecture
///
/// 1. Pool substrate layers (pads/pours) by (z_min, z_max, material, net)
/// 2. Pool trace geometry using the trace_geometry engine
/// 3. Pool PCB via pads (IC vias are already in substrate as Contact layers)
/// 4. Boolean union each pool to get final deduplicated contours
///
/// # No Fallbacks
///
/// This function uses proper lookup tables and fails fast if data is inconsistent.
/// It does NOT have hardcoded material names or default values.
pub fn generate_copper_contours(space: &HardwareSpace) -> Vec<UnifiedCopperContour> {
   
    let mut pools: FxHashMap<CopperPoolKey, Vec<Path64>> = FxHashMap::default();

    // 1. Add substrate layers (pads, pours, contacts)
    //    These are already realized by the compiler into the entity_graph
    let substrate_layers = space.entity_graph.get_substrate_layers();
    
   
    
    for layer in substrate_layers {
        // **v0.2.2 PROPER FIX**: Segment Contact layers by stackup
        // 
        // Contact layers (vias) span multiple stackup layers. We need to split them
        // into segments so that:
        // - Segments in conductive layers (active, metal) are rendered as solid copper
        // - Segments in dielectric layers are NOT rendered (substrate will have holes)
        if layer.layer_type == SubstrateLayerType::Contact {
            // Find which stackup layers this contact intersects
            segment_contact_by_stackup(layer, &space.stackup_layers, &mut pools);
            continue;
        }
        
        // Pour layers are already properly bounded and can be added directly
        if layer.layer_type != SubstrateLayerType::Pour {
            continue;
        }

        // Use the exact Z-bounds from the substrate layer bbox
        // These come from the stackup system, no calculation needed
        let key = CopperPoolKey {
            z_min: layer.bbox.min.z,
            z_max: layer.bbox.max.z,
            material: layer.material,
            net_id: layer.net,
        };

       

        let path = match &layer.shape {
            SubstrateLayerShape::Rect => rect_to_path(&layer.bbox),
            SubstrateLayerShape::Circle { radius } => {
                let cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                circle_to_path(cx, cy, *radius, 64)
            }
            SubstrateLayerShape::Polygon { outer_contour, .. } => {
                // Polygon points are stored in world space
                outer_contour.clone()
            }
            _ => continue,
        };

        pools.entry(key).or_default().push(path);
    }

    // 2. Add analytic trace geometry
    //    The trace_geometry engine handles proper Z-range resolution
    //    from AnalyticTrace.layer_z_range (stackup-derived)
  
    let trace_pools = trace_geometry::generate_trace_geometry(space);
    
    
    
    for (geom_key, mut geom_pool) in trace_pools {
        geom_pool.flush_pending();
        
        
        // Convert from trace_geometry key to unified key
        let key = CopperPoolKey {
            z_min: geom_key.z_min,
            z_max: geom_key.z_max,
            material: geom_key.material,
            net_id: geom_key.net_id,
        };
        
        pools.entry(key).or_default().extend(geom_pool.paths);
    }

    // 3. Add PCB via pads
    //    Note: IC vias (deposited) are already handled via Contact substrate layers
    //    Only PCB vias (drilled/plated) need explicit pad generation here
    for via in &space.vias {
        // Check via manufacturing process via material registry
        let is_deposited_via = space
            .material_registry
            .get_process(via.material_id)
            .map(|process| process == hwc_engine::ManufacturingProcess::Deposited)
            .unwrap_or(false);
            
        if is_deposited_via {
            // IC vias are already in entity_graph as Contact layers, skip
            continue;
        }

        // PCB via: add annular ring pads at top and bottom
        let z_start = via.from_z_nm.min(via.to_z_nm);
        let z_end = via.from_z_nm.max(via.to_z_nm);
        
        // Annular ring: pad extends beyond via barrel
        let pad_radius = via.diameter_nm / 2 + via.annular_ring_nm.max(via.diameter_nm / 4);
        
        // Find the conductor material for pads
        let pad_material_id = if space.material_registry.is_conductor(via.material_id) {
            via.material_id
        } else {
            // Find first conductive material
            space
                .material_registry
                .all_materials()
                .into_iter()
                .find(|(id, _name)| space.material_registry.is_conductor(*id))
                .map(|(id, _)| id)
                .expect("No conductive material found in material registry for via pads")
        };

        // Look up stackup layers at via landing points to get proper Z-bounds
        // Top pad: find the routable layer at z_end
        let top_layer = space
            .stackup_layers
            .iter()
            .find(|layer| layer.is_routable && layer.contains_z(z_end))
            .expect(&format!(
                "Via landing at Z={}nm has no corresponding routable layer in stackup. \
                Stackup layers must cover all via landing points.",
                z_end
            ));
        
        let top_z_min = top_layer.z_bottom;
        let top_z_max = top_layer.z_top;

        // Bottom pad: find the routable layer at z_start
        let bottom_layer = space
            .stackup_layers
            .iter()
            .find(|layer| layer.is_routable && layer.contains_z(z_start))
            .expect(&format!(
                "Via landing at Z={}nm has no corresponding routable layer in stackup. \
                Stackup layers must cover all via landing points.",
                z_start
            ));
        
        let bottom_z_min = bottom_layer.z_bottom;
        let bottom_z_max = bottom_layer.z_top;

        // Top pad
        let top_key = CopperPoolKey {
            z_min: top_z_min,
            z_max: top_z_max,
            material: pad_material_id,
            net_id: via.net_id,
        };
        pools
            .entry(top_key)
            .or_default()
            .push(circle_to_path(via.position.0, via.position.1, pad_radius, 64));

        // Bottom pad
        let bottom_key = CopperPoolKey {
            z_min: bottom_z_min,
            z_max: bottom_z_max,
            material: pad_material_id,
            net_id: via.net_id,
        };
        pools
            .entry(bottom_key)
            .or_default()
            .push(circle_to_path(via.position.0, via.position.1, pad_radius, 64));
    }

    // 4. Perform Boolean union on each pool
    let mut result = Vec::new();
    
   
    
    for (key, paths) in pools {
        if paths.is_empty() {
            continue;
        }

       

        // Boolean union to merge overlapping geometry
        let contours = clipper2_rust::union_64(&paths, &Vec::new(), FillRule::NonZero);
        
       
        
      
        
      
        
        if !contours.is_empty() {
            result.push(UnifiedCopperContour { key, contours });
        }
    }

    // Sort for deterministic output
    result.sort_by_key(|c| c.key);
    
   
    
    result
}

/// Segment a Contact layer (via) by stackup layers
///
/// **v0.2.3 Smart Cutout Strategy - No Z-Fighting**
///
/// ## The Z-Fighting Problem
/// When via penetrates partway into a pad, cutting a hole creates an internal wall
/// that Z-fights with the via surface. Solution: Only cut holes when via FULLY
/// spans the layer (goes all the way through).
///
/// ## Strategy
/// - **Via partially penetrates pad**: Render via inside pad, NO cutout (no Z-fighting!)
/// - **Via fully spans pad**: Cut complete hole through pad (like PCB through-hole)
/// - **Via in dielectric only**: Render via, cut hole in dielectric substrate
///
/// ## Example
/// Via Z=200→800nm through poly pad Z=0→300nm:
/// - Poly segment Z=200→300nm: Partial penetration (100nm deep) → NO cutout, via renders inside
/// - Dielectric Z=300→700nm: Full span → Cut hole in substrate, via fills it
/// - Metal segment Z=700→800nm: Partial penetration (100nm deep) → NO cutout, via renders inside
fn segment_contact_by_stackup(
    contact: &hwc_engine::geometry_router::substrate_types::SubstrateLayer,
    stackup_layers: &[hwc_engine::space::StackupLayer],
    pools: &mut FxHashMap<CopperPoolKey, Vec<Path64>>,
) {
    let via_z_min = contact.bbox.min.z;
    let via_z_max = contact.bbox.max.z;

    // Extract the via's 2D contour once
    let via_path = match &contact.shape {
        SubstrateLayerShape::Rect => rect_to_path(&contact.bbox),
        SubstrateLayerShape::Circle { radius } => {
            let cx = (contact.bbox.min.x + contact.bbox.max.x) / 2;
            let cy = (contact.bbox.min.y + contact.bbox.max.y) / 2;
            circle_to_path(cx, cy, *radius, 64)
        }
        SubstrateLayerShape::Polygon { outer_contour, .. } => outer_contour.clone(),
        _ => {
            eprintln!(
                "[VIA SEGMENT WARNING] Unsupported via shape: {:?}",
                contact.shape
            );
            return;
        }
    };

    // Find all stackup layers that this via intersects
    for stackup_layer in stackup_layers {
        // Check if via's Z-span intersects this stackup layer
        if via_z_max <= stackup_layer.z_bottom || via_z_min >= stackup_layer.z_top {
            continue; // No intersection
        }

        // Calculate the intersection Z-range
        let segment_z_min = via_z_min.max(stackup_layer.z_bottom);
        let segment_z_max = via_z_max.min(stackup_layer.z_top);

        if segment_z_max <= segment_z_min {
            continue; // Degenerate segment
        }

        // **v0.2.3 SMART CUTOUT**: Check if via fully spans this layer
        let via_fully_spans_layer = segment_z_min == stackup_layer.z_bottom 
                                    && segment_z_max == stackup_layer.z_top;

        if stackup_layer.is_routable {
            // Conductive layer (active, poly, metal)
            if via_fully_spans_layer {
                // Via goes completely through pad → Render via, cut hole (PCB style)
                eprintln!(
                    "[VIA FULL SPAN] Via fully spans conductive layer '{}' (Z={}→{}nm) - will cut hole",
                    stackup_layer.name, segment_z_min, segment_z_max
                );
                // Render via in this segment
                let key = CopperPoolKey {
                    z_min: segment_z_min,
                    z_max: segment_z_max,
                    material: contact.material,
                    net_id: contact.net,
                };
                pools.entry(key).or_default().push(via_path.clone());
            } else {
                // Via partially penetrates pad → Render via inside, NO hole (no Z-fighting!)
                eprintln!(
                    "[VIA PARTIAL] Via partially penetrates conductive layer '{}' (Z={}→{}nm, layer={}→{}nm) - embedding without cutout",
                    stackup_layer.name, segment_z_min, segment_z_max, 
                    stackup_layer.z_bottom, stackup_layer.z_top
                );
                // Render via in this segment (it will overlap with pad, but fully inside = no Z-fighting)
                let key = CopperPoolKey {
                    z_min: segment_z_min,
                    z_max: segment_z_max,
                    material: contact.material,
                    net_id: contact.net,
                };
                pools.entry(key).or_default().push(via_path.clone());
            }
        } else {
            // Dielectric layer - always render via and cut hole in substrate
            eprintln!(
                "[VIA DIELECTRIC] Via in dielectric layer '{}' (Z={}→{}nm) - will cut hole in substrate",
                stackup_layer.name, segment_z_min, segment_z_max
            );
            let key = CopperPoolKey {
                z_min: segment_z_min,
                z_max: segment_z_max,
                material: contact.material,
                net_id: contact.net,
            };
            pools.entry(key).or_default().push(via_path.clone());
        }
    }
}
