//! Stage 4: Conductive interconnect pour mesh resistance and substrate capacitance extraction.

use rustc_hash::FxHashMap;

use super::geometry::{find_dielectric_below, find_stackup_layer, find_stackup_layer_by_name, is_occluded_from_substrate};
use super::types::{ExtractedClusterNode, ExtractionConfig, EPS_0};
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

        let net_name = match &pour.net {
            Some(n) => n.as_str(),
            None => continue,
        };

        let stackup_layer_opt = if !pour.layer_name.is_empty() {
            find_stackup_layer_by_name(space, pour.layer_name.as_str())
        } else {
            find_stackup_layer(space, &pour.material_name, pour.z_bottom_nm)
        };

        if let Some(stackup_layer) = stackup_layer_opt {
            let pour_layer_name = stackup_layer.name.as_str();

            if let Some(ref bb) = pour.bbox {
                let width_nm = (bb.max.x - bb.min.x).abs() as f64;
                let length_nm = (bb.max.y - bb.min.y).abs() as f64;
                let area_m2 = (width_nm * 1e-9) * (length_nm * 1e-9);

                // Substrate Capacitance (only for non-device-body pours and unoccluded layers)
                let is_device_body = matches!(role, super::types::PourRole::DeviceTerminal { .. });
                let is_device_layer = space
                    .get_layer_by_name(&pour.layer_name)
                    .map(|l| l.is_device_layer)
                    .unwrap_or(false);
                let is_occluded = is_occluded_from_substrate(space, pour);

                if !is_device_body && !is_device_layer && !is_occluded {
                    if let Some((_, epsilon_r)) = find_dielectric_below(space, pour.z_bottom_nm as f64) {
                        let z_metal_m = pour.z_bottom_nm as f64 * 1e-9;
                        let z_substrate_m = space.get_substrate_z_nm() as f64 * 1e-9;
                        let d_m = (z_metal_m - z_substrate_m).max(10e-9);
                        let capacitance_f = EPS_0 * epsilon_r * (area_m2 / d_m);

                        if net_name != substrate_net && capacitance_f > 1e-17 {
                            // Determine the node for this capacitance attachment.
                            // A pour co-spatial with a pad mask layer pour (from the pad() PCell)
                            // is the physical metal pad polygon — its node is canonically n{Net}_pad.
                            // This is purely geometric/typed: no name-string inspection.
                            let is_metal_pad = space.pours.iter().any(|other| {
                                other.net.as_deref() == Some(net_name)
                                    && super::geometry::classify_pour(space, other)
                                        == super::types::PourRole::ExternalPad { net: net_name.into() }
                                    && other.bbox.as_ref().zip(pour.bbox.as_ref()).map_or(false, |(ob, pb)| {
                                        // Overlapping bbox = same pad footprint
                                        ob.min.x <= pb.max.x && ob.max.x >= pb.min.x
                                            && ob.min.y <= pb.max.y && ob.max.y >= pb.min.y
                                    })
                            });

                            let node_name: String = if is_metal_pad || matches!(&role, super::types::PourRole::ExternalPad { .. }) {
                                format!("n{}_pad", net_name)
                            } else {
                                // Interconnect strap: look for an extracted cluster node enclosed in this pour
                                let enclosed_node = extracted_layer_nodes
                                    .get(&(net_name.to_string(), pour_layer_name.to_string()))
                                    .and_then(|nodes| {
                                        nodes.iter().find(|n| {
                                            n.centroid.0 >= bb.min.x as f64
                                                && n.centroid.0 <= bb.max.x as f64
                                                && n.centroid.1 >= bb.min.y as f64
                                                && n.centroid.1 <= bb.max.y as f64
                                        }).map(|n| n.node.clone())
                                    });

                                if let Some(node) = enclosed_node {
                                    node
                                } else {
                                    let pour_center = super::geometry::get_bbox_centroid(pour.bbox.as_ref());
                                    super::routes::find_nearest_cluster_node(
                                        extracted_layer_nodes,
                                        net_name,
                                        pour_layer_name,
                                        pour_center,
                                        ExtractionConfig::default().via_cluster_radius_nm,
                                    )
                                    .unwrap_or_else(|| format!("n{}_{}", net_name, pour_layer_name))
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
                }

                // Interconnect Bus / Conductor Sheet Resistance 2D Manhattan Grid Mesh
                if let Some(material_props) = space.material_registry.get_physical_props_by_name(&pour.material_name) {
                    if let Some(resistivity) = material_props.get("resistivity") {
                        let thickness_m = stackup_layer.thickness as f64 * 1e-9;
                        if thickness_m > 0.0 {
                            let sheet_resistance = resistivity / thickness_m;

                            if let Some(layer_nodes) = extracted_layer_nodes.get(&(net_name.to_string(), pour_layer_name.to_string())) {
                                let enclosed_nodes: Vec<&ExtractedClusterNode> = layer_nodes
                                    .iter()
                                    .filter(|cn| {
                                        cn.centroid.0 >= bb.min.x as f64
                                            && cn.centroid.0 <= bb.max.x as f64
                                            && cn.centroid.1 >= bb.min.y as f64
                                            && cn.centroid.1 <= bb.max.y as f64
                                    })
                                    .collect();

                                if enclosed_nodes.len() >= 2 {
                                    // 2D Manhattan Grid Mesher for N x M Contact Arrays
                                    let tol = 50.0;
                                    let mut unique_xs: Vec<f64> = Vec::new();
                                    let mut unique_ys: Vec<f64> = Vec::new();

                                    for node in &enclosed_nodes {
                                        let (nx, ny) = node.centroid;
                                        if !unique_xs.iter().any(|&x| (x - nx).abs() < tol) {
                                            unique_xs.push(nx);
                                        }
                                        if !unique_ys.iter().any(|&y| (y - ny).abs() < tol) {
                                            unique_ys.push(ny);
                                        }
                                    }

                                    unique_xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                                    unique_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                                    let num_cols = unique_xs.len();
                                    let num_rows = unique_ys.len();

                                    let mut grid: Vec<Vec<Option<&ExtractedClusterNode>>> = vec![vec![None; num_cols]; num_rows];
                                    for node in &enclosed_nodes {
                                        let (nx, ny) = node.centroid;
                                        let col_idx = unique_xs.iter().position(|&x| (x - nx).abs() < tol).unwrap_or(0);
                                        let row_idx = unique_ys.iter().position(|&y| (y - ny).abs() < tol).unwrap_or(0);
                                        grid[row_idx][col_idx] = Some(node);
                                    }

                                    // Horizontal mesh resistors (between adjacent columns)
                                    let effective_row_width = (length_nm / num_rows as f64).max(1.0);
                                    for r in 0..num_rows {
                                        for c in 0..(num_cols - 1) {
                                            if let (Some(node_a), Some(node_b)) = (grid[r][c], grid[r][c + 1]) {
                                                let seg_dist = (node_b.centroid.0 - node_a.centroid.0).abs();
                                                if seg_dist > 0.0 {
                                                    let num_squares = seg_dist / effective_row_width;
                                                    let r_val = sheet_resistance * num_squares;
                                                    if r_val > 1e-6 {
                                                        graph.parasitics.push(ParasiticElement::TraceResistor {
                                                            name: format!("Rbus_{}_{}_h_{}_{}", pour.name, pour_layer_name, r, c),
                                                            node_a: node_a.node.clone(),
                                                            node_b: node_b.node.clone(),
                                                            value_ohms: r_val,
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Vertical mesh resistors (between adjacent rows)
                                    let effective_col_width = (width_nm / num_cols as f64).max(1.0);
                                    for c in 0..num_cols {
                                        for r in 0..(num_rows - 1) {
                                            if let (Some(node_a), Some(node_b)) = (grid[r][c], grid[r + 1][c]) {
                                                let seg_dist = (node_b.centroid.1 - node_a.centroid.1).abs();
                                                if seg_dist > 0.0 {
                                                    let num_squares = seg_dist / effective_col_width;
                                                    let r_val = sheet_resistance * num_squares;
                                                    if r_val > 1e-6 {
                                                        graph.parasitics.push(ParasiticElement::TraceResistor {
                                                            name: format!("Rbus_{}_{}_v_{}_{}", pour.name, pour_layer_name, r, c),
                                                            node_a: node_a.node.clone(),
                                                            node_b: node_b.node.clone(),
                                                            value_ohms: r_val,
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Connect external pad surface to top mesh node
                                    let is_external_pad = match role {
                                        super::types::PourRole::ExternalPad { .. } => true,
                                        _ => pour.name == net_name,
                                    };

                                    if is_external_pad {
                                        if !enclosed_nodes.is_empty() && !enclosed_nodes.iter().any(|n| n.node == net_name) {
                                            let pad_center = super::geometry::get_bbox_centroid(pour.bbox.as_ref());
                                            if let Some(nearest) = enclosed_nodes.iter().min_by(|a, b| {
                                                let d_a = super::geometry::distance_2d(pad_center, a.centroid);
                                                let d_b = super::geometry::distance_2d(pad_center, b.centroid);
                                                d_a.partial_cmp(&d_b).unwrap_or(std::cmp::Ordering::Equal)
                                            }) {
                                                let r_pad = (sheet_resistance * 0.05).max(1.0e-3);
                                                graph.parasitics.push(ParasiticElement::TraceResistor {
                                                    name: format!("Rbus_{}_to_{}", pour.name, net_name),
                                                    node_a: nearest.node.clone(),
                                                    node_b: net_name.to_string(),
                                                    value_ohms: r_pad,
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
}
