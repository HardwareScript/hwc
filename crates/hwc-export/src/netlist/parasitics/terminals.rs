//! Stage 5: Intent-driven mapping of device terminals to physical layout nodes.

use rustc_hash::FxHashMap;

use super::geometry::{distance_2d, find_stackup_layer, get_bbox_centroid};
use super::types::ExtractedClusterNode;
use crate::netlist::types::PhysicalNetlistGraph;
use hwc_compiler::alignment::PhysicalNetlist;
use hwc_engine::HardwareSpace;

/// Stage 5: Map schematic/layout device terminals to the closest physical interface nodes on corresponding layers.
pub fn map_device_terminals(
    space: &HardwareSpace,
    physical_netlist: Option<&PhysicalNetlist>,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    if let Some(netlist) = physical_netlist {
        for device in &netlist.devices {
            let mut mapped_terminals = device.terminals.clone();

            for (term_name, term_net) in &device.terminals {
                let term_pours: Vec<_> = space
                    .pours
                    .iter()
                    .filter(|p| {
                        p.device_binding.as_ref().map_or(false, |b| {
                            b.device_name == device.name && b.terminals.contains(term_name)
                        })
                    })
                    .collect();

                let mut best_node: Option<String> = None;
                let mut min_dist = f64::MAX;

                for pour in &term_pours {
                    let pour_centroid = get_bbox_centroid(pour.bbox.as_ref());
                    if let Some(stackup_layer) = find_stackup_layer(space, &pour.material_name, pour.z_bottom_nm) {
                        let layer_name = &stackup_layer.name;
                        if let Some(nodes) = extracted_layer_nodes.get(&(term_net.to_string(), layer_name.to_string())) {
                            for n in nodes {
                                let d = distance_2d(pour_centroid, n.centroid);
                                if d < min_dist {
                                    min_dist = d;
                                    best_node = Some(n.node.clone());
                                }
                            }
                        }
                    }
                }

                if let Some(node) = best_node {
                    mapped_terminals.insert(term_name.clone(), node);
                }
            }

            for (term_name, node) in mapped_terminals {
                graph.device_nodes.insert((device.name.to_string(), term_name.to_string()), node);
            }
        }
    }
}
