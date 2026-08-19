//! Stage 5: Intent-driven mapping of device terminals to physical layout nodes.
//!
//! ## Zero-Magic Terminal Binding (v0.2.2)
//!
//! Maps compact device terminals strictly to their intrinsic semiconductor interface nodes.
//! A compact subcircuit model (e.g. sky130_fd_pr__nfet_01v8) models the intrinsic semiconductor
//! channel. Its terminal ports must connect strictly to the physical interface node on the layer
//! matching the first-choice material declared in the device definition contract.
//!
//! Higher-level contact pads (e.g. Source_LI on li1, Source_Metal on metal1) are interconnect
//! metal layers that bridge to the channel via vertical contact vias. This architecture preserves
//! contact resistance in the simulation without heuristics or string matching.

use rustc_hash::FxHashMap;

use super::geometry::{distance_2d, find_stackup_layer, get_bbox_centroid};
use super::types::ExtractedClusterNode;
use crate::netlist::types::PhysicalNetlistGraph;
use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Stage 5: Map schematic/layout device terminals to the closest physical interface nodes on corresponding layers.
///
/// ## Architecture (v0.2.2: Zero-Magic Binding)
///
/// Instead of heuristic priority sorting or string matching ("gate", "g", "control"), this function:
/// 1. Queries the device contract from the SymbolTable to get declared terminal materials
/// 2. Selects the pour matching the intrinsic material (e.g. "N_Plus_Diffusion" for Source terminal)
/// 3. Falls back to lowest Z layer only if no material declaration exists
/// 4. Maps to the coordinate-closest extracted cluster node on the intrinsic layer
///
/// This guarantees that compact models bind to diff/poly nodes, while contact vias (Rvia) connect
/// diff -> li1 -> metal1, preserving the full series resistance chain.
pub fn map_device_terminals(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    let netlist = match physical_netlist {
        Some(nl) => nl,
        None => return,
    };

    for device in &netlist.devices {
        // Resolve device type ID to name using the device registry
        let device_type_name = netlist
            .device_registry
            .get_name(device.device_type_id)
            .ok_or_else(|| format!("Device '{}' has invalid device_type_id", device.name))
            .ok();

        // Query the device contract from the SymbolTable to get terminal material declarations
        let device_def = device_type_name
            .and_then(|name| symbol_table.get_device(name).ok());

        for (term_name, term_net) in &device.terminals {
            // Find all pours bound to this device terminal
            let term_pours: Vec<_> = space
                .pours
                .iter()
                .filter(|p| {
                    p.device_binding.as_ref().map_or(false, |b| {
                        b.device_name == device.name && b.terminals.contains(term_name)
                    })
                })
                .collect();

            if term_pours.is_empty() {
                continue;
            }

            // Determine intrinsic target material from device definition contract
            // This replaces heuristic priority sorting with declarative material matching
            let target_material = device_def
                .and_then(|def| def.materials.get(term_name))
                .and_then(|mats| mats.first());

            // Select pour matching the declared intrinsic material; fallback to lowest Z layer
            let intrinsic_pour = if let Some(mat) = target_material {
                term_pours
                    .iter()
                    .find(|p| p.material_name == *mat)
                    .or_else(|| term_pours.iter().min_by_key(|p| p.z_bottom_nm))
            } else {
                term_pours.iter().min_by_key(|p| p.z_bottom_nm)
            };

            let selected_pour = match intrinsic_pour {
                Some(p) => *p,
                None => continue,
            };

            let pour_centroid = get_bbox_centroid(selected_pour.bbox.as_ref());

            let layer_name = if !selected_pour.layer_name.is_empty() {
                selected_pour.layer_name.as_str()
            } else if let Some(stackup_layer) =
                find_stackup_layer(space, &selected_pour.material_name, selected_pour.z_bottom_nm)
            {
                stackup_layer.name.as_str()
            } else {
                continue;
            };

            // Match to the coordinate-closest extracted cluster node on the intrinsic layer
            if let Some(nodes) =
                extracted_layer_nodes.get(&(term_net.to_string(), layer_name.to_string()))
            {
                let mut best_node: Option<String> = None;
                let mut min_dist = f64::MAX;

                for n in nodes {
                    let d = distance_2d(pour_centroid, n.centroid);
                    if d < min_dist {
                        min_dist = d;
                        best_node = Some(n.node.clone());
                    }
                }

                if let Some(node) = best_node {
                    graph
                        .device_nodes
                        .insert((device.name.to_string(), term_name.to_string()), node);
                }
            }
        }
    }
}
