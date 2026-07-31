//! Refinement pipeline: Legalization → Compaction → Miter Pass

use super::types::GeometryRouter;
use crate::geometry::{Point3D, TraceSegment};
use crate::geometry_router::compaction::Compactor;
use crate::geometry_router::legalizer::Legalizer;
use crate::geometry_router::miter_pass::MiterEngine;
use crate::geometry_router::spatial_index::DynamicSpatialIndex;
use crate::geometry_router::types::RouteResult;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Apply post-routing refinement pipeline to a RouteResult.
    pub(crate) fn apply_refinement_pipeline(&self, result: &mut RouteResult) {
        eprintln!("[REFINEMENT PIPELINE DEBUG] Starting refinement pipeline");
        
        let min_clearance_nm = self
            .constraints
            .fabrication
            .as_ref()
            .expect("BUG: Fabrication constraints required for route refinement pipeline. \
                     Ensure the profile defines 'trace.min_spacing'.")
            .min_trace_spacing_nm;

        // --- Stage 1: Legalization ---
        let mut all_segments: Vec<TraceSegment> = Vec::new();
        let mut all_net_ids: Vec<NetId> = Vec::new();

        for (&net_id, paths) in &result.paths {
            for path in paths {
                for window in path.windows(2) {
                    eprintln!("[REFINEMENT DEBUG] Net {:?}: Converting segment ({},{},{}) -> ({},{},{})", 
                        net_id, window[0].x, window[0].y, window[0].z, window[1].x, window[1].y, window[1].z);
                    all_segments.push(TraceSegment::new(
                        window[0],
                        window[1],
                        self.trace_width_nm,
                        self.routing_material_id,
                    ));
                    all_net_ids.push(net_id);
                }
            }
        }

        if !all_segments.is_empty() {
            eprintln!("[REFINEMENT DEBUG] About to legalize {} segments", all_segments.len());
            
            // Build a properly configured layer-aware spatial index
            let mut spatial_index = DynamicSpatialIndex::new();
            
            // Configure layer Z-ranges from the entity graph
            if let Some(z_ranges) = self.entity_graph.spatial().layer_z_ranges() {
                eprintln!("[REFINEMENT DEBUG] Configuring spatial index with {} layer Z-ranges", z_ranges.len());
                spatial_index.set_layer_z_ranges(&z_ranges);
            } else {
                eprintln!("[REFINEMENT WARNING] No layer Z-ranges configured - all segments will be in fallback bucket!");
            }
            
            // Insert segments into spatial index
            for (idx, seg) in all_segments.iter().enumerate() {
                let net_id = all_net_ids.get(idx).copied().unwrap_or(NetId::UNCONNECTED);
                let net_idx = net_id.raw() as usize;
                let thickness_nm = self.material_registry
                    .get_material(seg.material_id)
                    .map(|m| m.thickness_nm)
                    .unwrap_or_else(|| {
                        panic!(
                            "FATAL: Material id={} has zero thickness — must be declared in PDK",
                            seg.material_id
                        )
                    });
                
                spatial_index.insert(crate::geometry_router::spatial_index::IndexedSegment::new(
                    hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx,
                        seg_idx: idx,
                    },
                    idx,
                    net_id,
                    seg,
                    seg.start.z,
                    thickness_nm,
                ));
            }
            
            let legalizer = Legalizer::new(min_clearance_nm);
            let (legalized_segments, legalized_net_ids) = legalizer.legalize(
                &all_segments,
                &all_net_ids,
                &self.material_registry,
                &spatial_index,
                10, // max iterations
            );

            eprintln!("[REFINEMENT DEBUG] After legalization: {} segments", legalized_segments.len());
            for (idx, seg) in legalized_segments.iter().enumerate().take(4) {
                eprintln!("[REFINEMENT DEBUG]   Segment {}: ({},{},{}) -> ({},{},{})", 
                    idx, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z);
            }

            // --- Stage 2: Compaction ---
            let compactor = Compactor::new(min_clearance_nm);
            let moves =
                compactor.compact(&legalized_segments, &legalized_net_ids, &Default::default());
            let compacted_segments = Compactor::apply_moves(&legalized_segments, &moves);

            eprintln!("[REFINEMENT DEBUG] After compaction: {} segments", compacted_segments.len());
            for (idx, seg) in compacted_segments.iter().enumerate().take(4) {
                eprintln!("[REFINEMENT DEBUG]   Segment {}: ({},{},{}) -> ({},{},{})", 
                    idx, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z);
            }

            // --- Stage 3: Miter Pass ---
            let miter_engine = MiterEngine::new(self.trace_width_nm);

            // Rebuild paths from compacted segments, grouped by net
            let mut refined_paths: FxHashMap<NetId, Vec<Vec<Point3D>>> = FxHashMap::default();
            for (idx, seg) in compacted_segments.iter().enumerate() {
                let net_id = legalized_net_ids[idx];
                let entry = refined_paths.entry(net_id).or_default();

                let continued = entry
                    .iter_mut()
                    .find(|path| path.last().is_some_and(|last| *last == seg.start));

                if let Some(path) = continued {
                    path.push(seg.end);
                } else {
                    entry.push(vec![seg.start, seg.end]);
                }
            }

            for paths in refined_paths.values_mut() {
                // v0.2.0 FIX: Miter pass is now applied in post_process.rs with via-awareness.
                // Applying it here causes double-mitering where the second pass incorrectly
                // identifies the first miter points as via endpoints. DISABLED.
                // miter_engine.apply_to_paths(paths);
            }

            // Merge paths: combine segments that share endpoints into single paths
            for (net_id, paths) in refined_paths {
                let mut merged: Vec<Vec<Point3D>> = Vec::new();
                let mut remaining = paths;

                while let Some(mut current) = remaining.pop() {
                    let mut changed = true;
                    while changed {
                        changed = false;
                        for i in (0..remaining.len()).rev() {
                            let other = &remaining[i];
                            if let Some(&current_end) = current.last() {
                                if other.first() == Some(&current_end) {
                                    current.extend_from_slice(&other[1..]);
                                    remaining.remove(i);
                                    changed = true;
                                } else if other.last() == current.first() {
                                    let mut new_path = other.clone();
                                    new_path.extend_from_slice(&current[1..]);
                                    current = new_path;
                                    remaining.remove(i);
                                    changed = true;
                                }
                            }
                        }
                    }
                    merged.push(current);
                }

                result.paths.insert(net_id, merged);
            }
        }
    }
}
