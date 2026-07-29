//! Re-registration of resolved routes into the physical entity graph.
//!
//! Ensures only final, detour-aware routes are registered in the physical
//! database, preventing "Double-Registration" bugs.

use crate::ir::errors::IrError;
use hwc_engine::netlist::NetId;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

/// Re-register all resolved routes from the analytic routes database
/// into the physical entity graph.
///
/// v0.1.9.1: This function ensures that only the final, detour-aware routes are registered
/// in the physical database, preventing "Double-Registration" bugs where the original
/// straight-line path and the detour path coexist (causing Clipper2 to weld them into a solid sheet).
pub fn re_register_resolved_routes(space: &mut HardwareSpace) -> Result<(), IrError> {
    eprintln!("[RE-REGISTER DEBUG] space.analytic_routes has {} routes:", space.analytic_routes.len());
    for (idx, trace) in space.analytic_routes.iter().enumerate() {
        eprintln!(
            "[RE-REGISTER DEBUG]   Route {} - net_name='{}', net_id={:?}, segments_count={}",
            idx, trace.net_name, trace.net_id, trace.segments.len()
        );
        for (seg_idx, seg) in trace.segments.iter().enumerate() {
            eprintln!(
                "[RE-REGISTER DEBUG]     seg {}: ({},{},{}) -> ({},{},{})",
                seg_idx, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
            );
        }
    }

    let net_ids_to_clear: Vec<_> = space
        .entity_graph
        .get_all_routes()
        .iter()
        .map(|(net_id, _)| *net_id)
        .collect();
    for net_id in net_ids_to_clear {
        space.entity_graph.clear_routes_for_net(net_id);
    }

    let mut unique_routes: FxHashMap<NetId, hwc_engine::AnalyticTrace> = FxHashMap::default();
    for trace in &space.analytic_routes {
        let entry = unique_routes.entry(trace.net_id);
        match entry {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(trace.clone());
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if trace.segments.len() > e.get().segments.len() {
                    
                    e.insert(trace.clone());
                } else {
                    eprintln!(
                        "[AUTO-ROUTER RE-REGISTER] Skipping redundant route for net_id={} ({} segments) as we already have {} segments",
                        trace.net_id.raw(),
                        trace.segments.len(),
                        e.get().segments.len()
                    );
                }
            }
        }
    }

    for (net_id, route_trace) in unique_routes {
        
        let trace_segments: Vec<hwc_engine::geometry::TraceSegment> = route_trace
            .segments
            .iter()
            .map(|line_seg| {
                hwc_engine::geometry::TraceSegment::new(
                    line_seg.start,
                    line_seg.end,
                    route_trace.cross_section.width_nm,
                    route_trace.material,
                )
            })
            .collect();

        if !trace_segments.is_empty() {
            space
                .entity_graph
                .register_trace_segments(net_id, trace_segments);
        }
    }

    Ok(())
}
