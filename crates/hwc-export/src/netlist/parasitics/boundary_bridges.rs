//! Boundary net bridging connecting top-level stimulus nets to physical pad landing nodes and pad under-layers.

use crate::netlist::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;
use super::types::ExtractedClusterNode;

pub fn emit_boundary_pad_bridges(
    space: &HardwareSpace,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    // Collect all net names from space netlist and classifications
    let mut net_names: Vec<String> = Vec::new();
    for net_id in space.netlist.all_net_ids() {
        if let Some(net) = space.netlist.get_net(net_id) {
            if !net_names.contains(&net.name.to_string()) {
                net_names.push(net.name.to_string());
            }
        }
    }
    for net_name in space.net_classifications.keys() {
        if !net_names.contains(&net_name.to_string()) {
            net_names.push(net_name.to_string());
        }
    }
    for pour in &space.pours {
        if let Some(ref n) = pour.net {
            if !net_names.contains(&n.to_string()) {
                net_names.push(n.to_string());
            }
        }
    }

    for net_name in &net_names {
        // 1. Collect pad mesh nodes for this net across layers
        let mut pad_mesh_nodes: Vec<(String, String)> = Vec::new(); // (layer_name, node_id)
        for ((n_name, layer), nodes) in extracted_layer_nodes {
            if n_name == net_name {
                for node in nodes {
                    pad_mesh_nodes.push((layer.clone(), node.node.clone()));
                }
            }
        }

        // 2. Collect route endpoints for this net from parasitics
        let mut route_starts: Vec<String> = Vec::new();
        let mut route_ends: Vec<String> = Vec::new();
        for p in &graph.parasitics {
            if let ParasiticElement::TraceResistor { name, node_a, node_b, .. } = p {
                if name.starts_with(&format!("Rtr_{}_", net_name)) {
                    if node_a.contains("_start") && !route_starts.contains(node_a) {
                        route_starts.push(node_a.clone());
                    }
                    if node_b.contains("_start") && !route_starts.contains(node_b) {
                        route_starts.push(node_b.clone());
                    }
                    if node_a.contains("_end") && !route_ends.contains(node_a) {
                        route_ends.push(node_a.clone());
                    }
                    if node_b.contains("_end") && !route_ends.contains(node_b) {
                        route_ends.push(node_b.clone());
                    }
                }
            }
        }

        // A. Bridge Top-Level Net -> Physical Pad Mesh Top Layer OR Trace End
        let already_has_stimulus = graph.parasitics.iter().any(|p| match p {
            ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                node_a == net_name || node_b == net_name
            }
            _ => false,
        });

        if !already_has_stimulus {
            // Find top-most metal layer pad node
            let top_pad_node = pad_mesh_nodes.iter().max_by_key(|(layer, _)| {
                space.stackup_layers.iter().position(|l| l.name == *layer).unwrap_or(0)
            });

            if let Some((_, pad_node)) = top_pad_node {
                graph.parasitics.push(ParasiticElement::TraceResistor {
                    name: format!("Rpad_stimulus_bridge_{}", net_name),
                    node_a: net_name.clone(),
                    node_b: pad_node.clone(),
                    value_ohms: 1.0e-4,
                });
            } else if let Some(trace_end) = route_ends.first() {
                graph.parasitics.push(ParasiticElement::TraceResistor {
                    name: format!("Rpad_stimulus_bridge_{}", net_name),
                    node_a: net_name.clone(),
                    node_b: trace_end.clone(),
                    value_ohms: 1.0e-4,
                });
            } else if let Some(trace_start) = route_starts.first() {
                graph.parasitics.push(ParasiticElement::TraceResistor {
                    name: format!("Rpad_stimulus_bridge_{}", net_name),
                    node_a: net_name.clone(),
                    node_b: trace_start.clone(),
                    value_ohms: 1.0e-4,
                });
            }
        }

        // B. Bridge Route Start -> Pad Under-Layer Mesh (GAP 2 Fix)
        for trace_start in &route_starts {
            for (layer, pad_node) in &pad_mesh_nodes {
                if trace_start.contains(layer.as_str()) && trace_start != pad_node {
                    let already_docked = graph.parasitics.iter().any(|p| match p {
                        ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                            (node_a == trace_start && node_b == pad_node)
                                || (node_b == trace_start && node_a == pad_node)
                        }
                        _ => false,
                    });
                    if !already_docked {
                        graph.parasitics.push(ParasiticElement::TraceResistor {
                            name: format!("Rpad_trace_dock_bridge_{}", net_name),
                            node_a: trace_start.clone(),
                            node_b: pad_node.clone(),
                            value_ohms: 1.0e-4,
                        });
                        break;
                    }
                }
            }
        }
    }
}
