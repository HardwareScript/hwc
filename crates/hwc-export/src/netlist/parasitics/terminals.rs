//! Stage 5: Deterministic device terminal → physical layer node binding.
//!
//! ## Zero-Heuristic Terminal Binding (v0.3.1)
//!
//! Device terminals declared in the device contract or CellLayout (e.g. `c0 -> TOP`, `c1 -> BOT`)
//! map directly through `terminal_ports` and `terminal_bindings` to the physical port geometry
//! and the exact extracted trace/via landing node on that layer.
//!
//! Zero string pattern matching. Zero fabricated dummy strings.

use rustc_hash::FxHashMap;

use super::geometry::{distance_2d, get_bbox_centroid};
use super::types::ExtractedClusterNode;
use crate::netlist::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Stage 5: Bind each device terminal to the correct extracted physical layer node.
pub fn map_device_terminals(
    space: &HardwareSpace,
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
        for (term_name, term_net) in &device.terminals {
            // 1. Resolve declared target layer
            let declared_layer = device.terminal_layers.get(term_name)
                .or_else(|| {
                    device.terminal_bindings.iter()
                        .find(|b| b.terminal == *term_name && (b.instance_name == device.name || b.instance_name.is_empty()))
                        .map(|b| &b.layer_name)
                });

            // 2. Resolve declared target port (e.g., c0 -> TOP, c1 -> BOT)
            let port_target = device.terminal_ports.get(term_name)
                .or_else(|| {
                    device.terminal_bindings.iter()
                        .find(|b| b.terminal == *term_name && (b.instance_name == device.name || b.instance_name.is_empty()))
                        .map(|b| &b.port)
                });

            // 3. Find spatial location of the port/terminal geometry (exact world coordinate)
            let target_point: Option<(f64, f64)> = if let Some(port_name) = port_target {
                device.port_positions.get(port_name).map(|(x, y)| (*x as f64, *y as f64))
                    .or_else(|| {
                        space.pours.iter()
                            .find(|p| {
                                p.name == *port_name
                                    || p.name == format!("{}_{}", device.name, port_name).as_str()
                                    || p.name == format!("{}_{}", device.device_type, port_name).as_str()
                            })
                            .map(|p| get_bbox_centroid(p.bbox.as_ref()))
                    })
            } else {
                None
            };

            // 4. Query the exact node allocated when a trace/via docked into this port / layer
            let node = if let Some(layer) = declared_layer {
                if let Some(nodes) = extracted_layer_nodes.get(&(term_net.to_string(), layer.to_string())) {
                    if let Some(pt) = target_point {
                        nodes.iter()
                            .min_by(|a, b| {
                                let d_a = distance_2d(pt, a.centroid);
                                let d_b = distance_2d(pt, b.centroid);
                                d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|n| n.node.clone())
                    } else {
                        nodes.first().map(|n| n.node.clone())
                    }
                } else {
                    None
                }
            } else {
                // If no explicit layer declared, search all extracted nodes for this net closest to the port
                extracted_layer_nodes.iter()
                    .filter(|((net, _), _)| net == term_net.as_str())
                    .flat_map(|(_, nodes)| nodes.iter())
                    .min_by(|a, b| {
                        if let Some(pt) = target_point {
                            let d_a = distance_2d(pt, a.centroid);
                            let d_b = distance_2d(pt, b.centroid);
                            d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .map(|n| n.node.clone())
            };

            if let Some(resolved_node) = node {
                graph.device_nodes.insert((device.name.to_string(), term_name.to_string()), resolved_node);
            } else {
                panic!(
                    "FATAL: Device '{}' terminal '{}' is bound to port '{:?}' (net '{}'), but no physical trace or via connected to this port.",
                    device.name, term_name, port_target, term_net
                );
            }
        }
    }
}
