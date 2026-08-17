//! Stage 4: Conductive interconnect pour mesh resistance and substrate capacitance extraction.

use rustc_hash::FxHashMap;

use super::geometry::{find_dielectric_below, find_stackup_layer};
use super::types::{ExtractedClusterNode, EPS_0};
use crate::netlist::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_engine::HardwareSpace;

/// Stage 4: Extract distributed substrate capacitance and interconnect bus sheet resistance.
pub fn extract_interconnect_pours(
    space: &HardwareSpace,
    substrate_net: &str,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    for pour in &space.pours {
        let role = super::geometry::classify_pour(space, pour);

        // Intent-driven exemption: if pour is bound to a device body, skip
        if let super::types::PourRole::DeviceTerminal { .. } = role {
            continue;
        }

        let net_name = match &pour.net {
            Some(n) => n.as_str(),
            None => continue,
        };

        if let Some(stackup_layer) = find_stackup_layer(space, &pour.material_name, pour.z_bottom_nm) {
            let pour_layer_name = stackup_layer.name.as_str();

            if let Some(ref bb) = pour.bbox {
                let width_nm = (bb.max.x - bb.min.x).abs() as f64;
                let length_nm = (bb.max.y - bb.min.y).abs() as f64;
                let area_m2 = (width_nm * 1e-9) * (length_nm * 1e-9);

                // Substrate Capacitance
                if let Some((sub_height_nm, epsilon_r)) = find_dielectric_below(space, pour.z_bottom_nm as f64) {
                    let sub_height_m = sub_height_nm * 1e-9;
                    let capacitance_f = EPS_0 * epsilon_r * (area_m2 / sub_height_m);

                    if net_name != substrate_net && capacitance_f > 1e-17 {
                        let node_name = match &role {
                            super::types::PourRole::ExternalPad { net } => net.to_string(),
                            _ => {
                                let pour_center = super::geometry::get_bbox_centroid(pour.bbox.as_ref());
                                if let Some(cluster_node) = super::routes::find_nearest_cluster_node(
                                    extracted_layer_nodes,
                                    net_name,
                                    pour_layer_name,
                                    pour_center,
                                    super::types::VIA_CLUSTER_RADIUS_NM,
                                ) {
                                    cluster_node
                                } else {
                                    format!("n{}_{}", net_name, pour_layer_name)
                                }
                            }
                        };

                        graph.parasitics.push(ParasiticElement::GroundCapacitance {
                            name: format!("Cgnd_pour_{}", pour.name),
                            node: node_name,
                            ref_node: substrate_net.to_string(),
                            value_farads: capacitance_f,
                        });
                    }
                }

                // Interconnect Bus Sheet Resistance Mesh
                if let Some(material_props) = space.material_registry.get_physical_props_by_name(&pour.material_name) {
                    if let Some(resistivity) = material_props.get("resistivity") {
                        let thickness_m = stackup_layer.thickness as f64 * 1e-9;
                        if thickness_m > 0.0 {
                            let sheet_resistance = resistivity / thickness_m;

                            if let Some(layer_nodes) = extracted_layer_nodes.get(&(net_name.to_string(), pour_layer_name.to_string())) {
                                let margin = 100.0;
                                let mut enclosed_nodes: Vec<&ExtractedClusterNode> = layer_nodes
                                    .iter()
                                    .filter(|cn| {
                                        cn.centroid.0 >= (bb.min.x as f64 - margin)
                                            && cn.centroid.0 <= (bb.max.x as f64 + margin)
                                            && cn.centroid.1 >= (bb.min.y as f64 - margin)
                                            && cn.centroid.1 <= (bb.max.y as f64 + margin)
                                    })
                                    .collect();

                                if enclosed_nodes.len() >= 2 {
                                    let is_horizontal_bus = width_nm >= length_nm;
                                    if is_horizontal_bus {
                                        enclosed_nodes.sort_by(|a, b| a.centroid.0.partial_cmp(&b.centroid.0).unwrap_or(std::cmp::Ordering::Equal));
                                    } else {
                                        enclosed_nodes.sort_by(|a, b| a.centroid.1.partial_cmp(&b.centroid.1).unwrap_or(std::cmp::Ordering::Equal));
                                    }

                                    for i in 0..(enclosed_nodes.len() - 1) {
                                        let node_a = enclosed_nodes[i];
                                        let node_b = enclosed_nodes[i + 1];

                                        let seg_dist_nm = if is_horizontal_bus {
                                            (node_b.centroid.0 - node_a.centroid.0).abs()
                                        } else {
                                            (node_b.centroid.1 - node_a.centroid.1).abs()
                                        };

                                        let bus_width_nm = if is_horizontal_bus { length_nm } else { width_nm };

                                        if bus_width_nm > 0.0 && seg_dist_nm > 0.0 {
                                            let num_squares = seg_dist_nm / bus_width_nm;
                                            let r_bus_seg = sheet_resistance * num_squares;

                                            if r_bus_seg > 0.001 {
                                                graph.parasitics.push(ParasiticElement::TraceResistor {
                                                    name: format!("Rbus_{}_{}", pour.name, i),
                                                    node_a: node_a.node.clone(),
                                                    node_b: node_b.node.clone(),
                                                    value_ohms: r_bus_seg,
                                                });
                                            }
                                        }
                                    }

                                    // Bridge bus terminal to the global net node if classified as PowerBus
                                    if let super::types::PourRole::PowerBus { .. } = role {
                                        let last_node = enclosed_nodes[enclosed_nodes.len() - 1];
                                        let bus_width_nm = if is_horizontal_bus { length_nm } else { width_nm };
                                        let rem_dist_nm = if is_horizontal_bus {
                                            (bb.max.x as f64 - last_node.centroid.0).abs()
                                        } else {
                                            (bb.max.y as f64 - last_node.centroid.1).abs()
                                        };
                                        if bus_width_nm > 0.0 && rem_dist_nm > 0.0 {
                                            let num_squares = rem_dist_nm / bus_width_nm;
                                            let r_bus_pad = sheet_resistance * num_squares;
                                            if r_bus_pad > 0.001 {
                                                graph.parasitics.push(ParasiticElement::TraceResistor {
                                                    name: format!("Rbus_{}_to_{}", pour.name, net_name),
                                                    node_a: last_node.node.clone(),
                                                    node_b: net_name.to_string(),
                                                    value_ohms: r_bus_pad,
                                                });
                                            }
                                        }
                                    }
                                } else if enclosed_nodes.is_empty() {
                                    if let super::types::PourRole::PowerBus { .. } | super::types::PourRole::InterconnectStrap { .. } = role {
                                        let num_squares = if width_nm >= length_nm {
                                            width_nm / length_nm.max(1.0)
                                        } else {
                                            length_nm / width_nm.max(1.0)
                                        };
                                        let r_val = sheet_resistance * num_squares;
                                        if r_val > 0.001 {
                                            let node_a = format!("n{}_{}", net_name, pour_layer_name);
                                            let node_b = format!("n{}_{}_strap", net_name, pour_layer_name);
                                            graph.parasitics.push(ParasiticElement::TraceResistor {
                                                name: format!("Rpour_{}", pour.name),
                                                node_a,
                                                node_b,
                                                value_ohms: r_val,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
