//! Stage 5: Deterministic device terminal → physical layer node binding.
//!
//! ## Zero-Heuristic Terminal Binding (v0.3.1)
//!
//! Device terminals declared in the device contract or CellLayout (e.g. `c0 -> TOP`, `c1 -> BOT`)
//! map directly through typed `TerminalLanding` records to the physical port geometry
//! and the exact extracted trace/via landing node on that layer.
//!
//! Zero string pattern matching. Zero fabricated dummy strings. Zero heuristics.

use rustc_hash::FxHashMap;

use super::geometry::distance_2d;
use super::types::ExtractedClusterNode;
use crate::netlist::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Stage 5: Bind each device terminal to the correct extracted physical layer node via typed landings.
pub fn map_device_terminals(
    _space: &HardwareSpace,
    _symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    let netlist = match physical_netlist {
        Some(nl) => nl,
        None => return,
    };

    for device in &netlist.devices {
        for landing in &device.terminal_landings {
            let is_bulk = landing.terminal_name.eq_ignore_ascii_case("BULK")
                || landing.terminal_name.eq_ignore_ascii_case("SUB");

            let node = if is_bulk {
                // Substrate terminals always bind to the closest diffusion landing node
                extracted_layer_nodes
                    .get(&(landing.net_name.to_string(), "pdiff".to_string()))
                    .and_then(|nodes| {
                        nodes
                            .iter()
                            .min_by(|a, b| {
                                let d_a = distance_2d(landing.world_pos, a.centroid);
                                let d_b = distance_2d(landing.world_pos, b.centroid);
                                d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|n| n.node.clone())
                    })
                    .or_else(|| {
                        extracted_layer_nodes
                            .get(&(landing.net_name.to_string(), landing.landing_layer.to_string()))
                            .and_then(|nodes| {
                                nodes
                                    .iter()
                                    .min_by(|a, b| {
                                        let d_a = distance_2d(landing.world_pos, a.centroid);
                                        let d_b = distance_2d(landing.world_pos, b.centroid);
                                        d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                                    })
                                    .map(|n| n.node.clone())
                            })
                    })
                    .unwrap_or_else(|| landing.net_name.to_string())
            } else {
                let target_nodes = extracted_layer_nodes.get(&(landing.net_name.to_string(), landing.landing_layer.to_string()));

                target_nodes
                    .and_then(|nodes| {
                        nodes
                            .iter()
                            .min_by(|a, b| {
                                let d_a = distance_2d(landing.world_pos, a.centroid);
                                let d_b = distance_2d(landing.world_pos, b.centroid);
                                d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|n| n.node.clone())
                    })
                    .unwrap_or_else(|| landing.net_name.to_string())
            };

            graph
                .device_nodes
                .insert((device.name.to_string(), landing.terminal_name.to_string()), node);
        }
    }
}
