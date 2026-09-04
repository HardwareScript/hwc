//! Stage 2: Series trace resistance and microstrip ground capacitance extraction.

use rustc_hash::FxHashMap;

use super::geometry::{distance_2d, find_dielectric_below, find_pour_at_point};
use super::types::{ExtractedClusterNode, ExtractionConfig, EPS_0};
use crate::netlist::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_engine::HardwareSpace;

/// Helper to find nearest extracted node on a layer for a given point (x, y)
pub fn find_nearest_cluster_node(
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
    net_name: &str,
    layer_name: &str,
    point: (f64, f64),
    max_dist_nm: f64,
) -> Option<String> {
    if let Some(nodes) = extracted_layer_nodes.get(&(net_name.to_string(), layer_name.to_string())) {
        nodes
            .iter()
            .filter_map(|n| {
                let d = distance_2d(point, n.centroid);
                if d <= max_dist_nm {
                    Some((n.node.clone(), d))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(node, _)| node)
    } else {
        None
    }
}

/// Resolve the physical node corresponding to a route endpoint on a layer
pub fn resolve_route_endpoint_node(
    space: &HardwareSpace,
    extracted_layer_nodes: &mut FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
    net_name: &str,
    layer_name: &str,
    point: (f64, f64),
    trace_idx: usize,
    is_start: bool,
    config: &ExtractionConfig,
) -> String {
    // 1. Check if the point lands inside a pour on this layer
    if let Some(pour) = find_pour_at_point(space, net_name, layer_name, point) {
        let is_pad_pour = pour.name.contains("pad")
            || space.pours.iter().any(|other| {
                other.net.as_deref() == Some(net_name)
                    && other.name.contains("pad")
                    && other.bbox.as_ref().zip(pour.bbox.as_ref()).map_or(false, |(ob, pb)| {
                        ob.min.x <= pb.max.x && ob.max.x >= pb.min.x
                            && ob.min.y <= pb.max.y && ob.max.y >= pb.min.y
                    })
            });

        if is_pad_pour {
            return format!("n{}_pad", net_name);
        }

        if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, net_name, layer_name, point, config.via_landing_radius_nm) {
            return cluster_node;
        }
    }

    // 2. Check if near any via cluster on this layer
    if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, net_name, layer_name, point, config.via_landing_radius_nm) {
        return cluster_node;
    }

    // 3. Fallback: unique trace endpoint node
    let suffix = if is_start { "start" } else { "end" };
    let node = format!("n{}_{}_tr{}_{}", net_name, layer_name, trace_idx, suffix);
    let list = extracted_layer_nodes
        .entry((net_name.to_string(), layer_name.to_string()))
        .or_default();
    if !list.iter().any(|item| item.node == node) {
        list.push(ExtractedClusterNode {
            node: node.clone(),
            centroid: point,
        });
    }
    node
}

/// Stage 2: Extract series trace resistance and 1D microstrip ground capacitance.
pub fn extract_traces(
    space: &HardwareSpace,
    substrate_net: &str,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &mut FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
    config: &ExtractionConfig,
) {
    for (trace_idx, trace) in space.analytic_routes.iter().enumerate() {
        let total_segs = trace.segments.len();
        if total_segs == 0 {
            continue;
        }

        let net_name = &trace.net_name;
        let layer_name = &trace.layer_name;

        let start_point = (
            trace.segments[0].start.x as f64,
            trace.segments[0].start.y as f64,
        );
        let end_point = (
            trace.segments[total_segs - 1].end.x as f64,
            trace.segments[total_segs - 1].end.y as f64,
        );

        let start_node = resolve_route_endpoint_node(space, extracted_layer_nodes, net_name.as_str(), layer_name.as_str(), start_point, trace_idx, true, config);
        let end_node = resolve_route_endpoint_node(space, extracted_layer_nodes, net_name.as_str(), layer_name.as_str(), end_point, trace_idx, false, config);

        let mut prev_node = start_node;

        for (seg_num, segment) in trace.segments.iter().enumerate() {
            let node_end = if seg_num == total_segs - 1 {
                end_node.clone()
            } else {
                let seg_end_pt = (segment.end.x as f64, segment.end.y as f64);
                if let Some(pour) = find_pour_at_point(space, net_name.as_str(), layer_name.as_str(), seg_end_pt) {
                    let is_pad_pour = pour.name.contains("pad")
                        || space.pours.iter().any(|other| {
                            other.net.as_deref() == Some(net_name.as_str())
                                && other.name.contains("pad")
                                && other.bbox.as_ref().zip(pour.bbox.as_ref()).map_or(false, |(ob, pb)| {
                                    ob.min.x <= pb.max.x && ob.max.x >= pb.min.x
                                        && ob.min.y <= pb.max.y && ob.max.y >= pb.min.y
                                })
                        });

                    if is_pad_pour {
                        format!("n{}_pad", net_name)
                    } else if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, net_name.as_str(), layer_name.as_str(), seg_end_pt, config.via_landing_radius_nm) {
                        cluster_node
                    } else {
                        format!("n{}_{}_tr{}_seg{}", net_name, layer_name, trace_idx, seg_num + 1)
                    }
                } else if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, net_name.as_str(), layer_name.as_str(), seg_end_pt, config.via_landing_radius_nm) {
                    cluster_node
                } else {
                    format!("n{}_{}_tr{}_seg{}", net_name, layer_name, trace_idx, seg_num + 1)
                }
            };

            // Trace series resistance
            if let Some(material_props) = space.material_registry.get_physical_props(trace.material) {
                if let Some(resistivity) = material_props.get("resistivity") {
                    let thickness_m = trace.cross_section.thickness_nm as f64 * 1e-9;
                    let width_m = trace.cross_section.width_nm as f64 * 1e-9;
                    let cross_section_m2 = width_m * thickness_m;

                    let dx = (segment.end.x - segment.start.x) as f64;
                    let dy = (segment.end.y - segment.start.y) as f64;
                    let dz = (segment.end.z - segment.start.z) as f64;

                    let length_m = (dx * dx + dy * dy + dz * dz).sqrt() * 1e-9;

                    if cross_section_m2 > 0.0 && length_m > 0.0 {
                        let resistance_ohm = resistivity * (length_m / cross_section_m2);
                        if resistance_ohm > 0.0001 && prev_node != node_end {
                            graph.parasitics.push(ParasiticElement::TraceResistor {
                                name: format!("Rtr_{}_{}_{}", net_name, trace_idx, seg_num),
                                node_a: prev_node.clone(),
                                node_b: node_end.clone(),
                                value_ohms: resistance_ohm,
                            });
                        }
                    }

                    // Microstrip substrate ground capacitance
                    if let Some((sub_height_nm, epsilon_r)) = find_dielectric_below(space, segment.start.z as f64) {
                        let sub_height_m = sub_height_nm * 1e-9;
                        let area_m2 = width_m * length_m;
                        let capacitance_f = EPS_0 * epsilon_r * (area_m2 / sub_height_m);

                        if net_name.as_str() != substrate_net && node_end != substrate_net && capacitance_f > 1e-17 {
                            graph.parasitics.push(ParasiticElement::GroundCapacitance {
                                name: format!("Cgnd_{}_{}_{}", net_name, trace_idx, seg_num),
                                node: node_end.clone(),
                                ref_node: substrate_net.to_string(),
                                value_farads: capacitance_f,
                            });
                        }
                    }
                }
            }

            prev_node = node_end;
        }
    }
}
