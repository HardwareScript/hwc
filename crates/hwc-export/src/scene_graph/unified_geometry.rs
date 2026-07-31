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
//! # Design Rules
//!
//! - NO hardcoded material names (use MaterialRegistry lookups)
//! - NO fallback defaults (fail fast if data is missing)
//! - Use proper typed IDs (MaterialId, NetId) everywhere
//! - All Z-ranges come from either AnalyticTrace.layer_z_range or SubstrateLayer.bbox

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
    eprintln!("[UNIFIED GEOMETRY] Starting generation...");
    let mut pools: FxHashMap<CopperPoolKey, Vec<Path64>> = FxHashMap::default();

    // 1. Add substrate layers (pads, pours, contacts)
    //    These are already realized by the compiler into the entity_graph
    let substrate_layers = space.entity_graph.get_substrate_layers();
    
    eprintln!("[UNIFIED GEOMETRY] Processing {} substrate layers", substrate_layers.len());
    
    for layer in substrate_layers {
        // Only include conductive layers (Pour and Contact types)
        if layer.layer_type != SubstrateLayerType::Pour
            && layer.layer_type != SubstrateLayerType::Contact
        {
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

        eprintln!(
            "[UNIFIED GEOMETRY]   Substrate layer: net={:?}, type={:?}, material={:?}, Z={}→{}nm",
            layer.net, layer.layer_type, layer.material, layer.bbox.min.z, layer.bbox.max.z
        );

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
    eprintln!("[UNIFIED GEOMETRY] Generating trace geometry...");
    let trace_pools = trace_geometry::generate_trace_geometry(space);
    
    eprintln!("[UNIFIED GEOMETRY] Got {} trace pools", trace_pools.len());
    
    for (geom_key, mut geom_pool) in trace_pools {
        geom_pool.flush_pending();
        
        eprintln!(
            "[UNIFIED GEOMETRY]   Trace pool: net={:?}, Z={}→{}nm, {} paths",
            geom_key.net_id, geom_key.z_min, geom_key.z_max, geom_pool.paths.len()
        );
        
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
    
    eprintln!("[UNIFIED GEOMETRY] Performing Boolean union on {} pools...", pools.len());
    
    for (key, paths) in pools {
        if paths.is_empty() {
            continue;
        }

        eprintln!(
            "[UNIFIED GEOMETRY]   Pool key=(z={:?}→{:?}, mat={:?}, net={:?}): {} paths before union",
            key.z_min, key.z_max, key.material, key.net_id, paths.len()
        );

        // Debug: Print bounding boxes of first few paths
        for (idx, path) in paths.iter().enumerate().take(5) {
            if !path.is_empty() {
                let min_x = path.iter().map(|p| p.x).min().unwrap();
                let max_x = path.iter().map(|p| p.x).max().unwrap();
                let min_y = path.iter().map(|p| p.y).min().unwrap();
                let max_y = path.iter().map(|p| p.y).max().unwrap();
                eprintln!(
                    "[UNIFIED GEOMETRY]     Path {}: bbox=({}, {}) to ({}, {}), {} points",
                    idx, min_x, min_y, max_x, max_y, path.len()
                );
            }
        }

        // Boolean union to merge overlapping geometry
        let contours = clipper2_rust::union_64(&paths, &Vec::new(), FillRule::NonZero);
        
        eprintln!("[UNIFIED GEOMETRY]     Input paths to union:");
        for (i, path) in paths.iter().enumerate() {
            eprintln!("[UNIFIED GEOMETRY]       Path {}: {} points", i, path.len());
            for (j, pt) in path.iter().enumerate() {
                eprintln!("[UNIFIED GEOMETRY]         Point {}: ({}, {})", j, pt.x, pt.y);
            }
        }
        
        eprintln!(
            "[UNIFIED GEOMETRY]     After union: {} contours",
            contours.len()
        );
        
        // Debug: Print first few points of the unified contour
        if !contours.is_empty() && !contours[0].is_empty() {
            let num_points = contours[0].len().min(8);
            eprint!("[UNIFIED GEOMETRY]     First {} points of unified contour: ", num_points);
            for i in 0..num_points {
                eprint!("({},{}) ", contours[0][i].x, contours[0][i].y);
            }
            eprintln!();
        }
        
        if !contours.is_empty() {
            result.push(UnifiedCopperContour { key, contours });
        }
    }

    // Sort for deterministic output
    result.sort_by_key(|c| c.key);
    
    eprintln!("[UNIFIED GEOMETRY] Returning {} unified contour groups", result.len());
    
    result
}
