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
        let min_clearance_nm = self
            .constraints
            .fabrication
            .as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .unwrap_or(200_000);

        // --- Stage 1: Legalization ---
        let mut all_segments: Vec<TraceSegment> = Vec::new();
        let mut all_net_ids: Vec<NetId> = Vec::new();

        for (&net_id, paths) in &result.paths {
            for path in paths {
                for window in path.windows(2) {
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
            let spatial_index = DynamicSpatialIndex::new();
            let legalizer = Legalizer::new(min_clearance_nm);
            let (legalized_segments, legalized_net_ids) = legalizer.legalize(
                &all_segments,
                &all_net_ids,
                &self.material_registry,
                &spatial_index,
                10, // max iterations
            );

            // --- Stage 2: Compaction ---
            let compactor = Compactor::new(min_clearance_nm);
            let moves =
                compactor.compact(&legalized_segments, &legalized_net_ids, &Default::default());
            let compacted_segments = Compactor::apply_moves(&legalized_segments, &moves);

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
                miter_engine.apply_to_paths(paths);
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
