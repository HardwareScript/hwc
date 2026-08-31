//! Stage 3: Lateral sidewall coupling capacitance extraction between parallel traces.

use rustc_hash::FxHashMap;

use super::geometry::find_pour_at_point;
use super::routes::find_nearest_cluster_node;
use super::types::{ExtractedClusterNode, ExtractionConfig, EPS_0};
use crate::netlist::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_engine::HardwareSpace;

struct SegmentGeometry {
    net_name: String,
    node: String,
    layer_name: String,
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    width_nm: f64,
    thickness_nm: f64,
}

/// Stage 3: Extract lateral coupling capacitance between parallel traces.
pub fn extract_lateral_coupling(
    space: &HardwareSpace,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
    config: &ExtractionConfig,
) {
    let mut segments: Vec<SegmentGeometry> = Vec::new();
    for (trace_idx, trace) in space.analytic_routes.iter().enumerate() {
        let total_segs = trace.segments.len();
        for (seg_num, segment) in trace.segments.iter().enumerate() {
            let node = if seg_num == total_segs - 1 {
                let end_point = (segment.end.x as f64, segment.end.y as f64);
                if let Some(pour) = find_pour_at_point(space, &trace.net_name, &trace.layer_name, end_point) {
                    match super::geometry::classify_pour(space, pour) {
                        super::types::PourRole::ExternalPad { net } => net.to_string(),
                        _ => {
                            if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, &trace.net_name, &trace.layer_name, end_point, config.via_landing_radius_nm) {
                                cluster_node
                            } else {
                                format!("n{}_{}_tr{}_end", trace.net_name, trace.layer_name, trace_idx)
                            }
                        }
                    }
                } else if let Some(cluster_node) = find_nearest_cluster_node(extracted_layer_nodes, &trace.net_name, &trace.layer_name, end_point, config.via_landing_radius_nm) {
                    cluster_node
                } else {
                    format!("n{}_{}_tr{}_end", trace.net_name, trace.layer_name, trace_idx)
                }
            } else {
                format!("n{}_{}_tr{}_seg{}", trace.net_name, trace.layer_name, trace_idx, seg_num + 1)
            };
            segments.push(SegmentGeometry {
                net_name: trace.net_name.to_string(),
                node,
                layer_name: trace.layer_name.to_string(),
                start_x: segment.start.x as f64,
                start_y: segment.start.y as f64,
                end_x: segment.end.x as f64,
                end_y: segment.end.y as f64,
                width_nm: trace.cross_section.width_nm as f64,
                thickness_nm: trace.cross_section.thickness_nm as f64,
            });
        }
    }

    let num_segs = segments.len();
    for i in 0..num_segs {
        for j in (i + 1)..num_segs {
            let seg_a = &segments[i];
            let seg_b = &segments[j];

            if seg_a.net_name == seg_b.net_name || seg_a.layer_name != seg_b.layer_name {
                continue;
            }

            let is_a_horizontal = (seg_a.start_y - seg_a.end_y).abs() < 1e-3;
            let is_b_horizontal = (seg_b.start_y - seg_b.end_y).abs() < 1e-3;
            let is_a_vertical = (seg_a.start_x - seg_a.end_x).abs() < 1e-3;
            let is_b_vertical = (seg_b.start_x - seg_b.end_x).abs() < 1e-3;

            let mut parallel_length_nm = 0.0;
            let mut edge_to_edge_spacing_nm = 0.0;

            if is_a_horizontal && is_b_horizontal {
                let center_dist_y = (seg_a.start_y - seg_b.start_y).abs();
                edge_to_edge_spacing_nm = center_dist_y - (seg_a.width_nm + seg_b.width_nm) / 2.0;

                let min_x_a = seg_a.start_x.min(seg_a.end_x);
                let max_x_a = seg_a.start_x.max(seg_a.end_x);
                let min_x_b = seg_b.start_x.min(seg_b.end_x);
                let max_x_b = seg_b.start_x.max(seg_b.end_x);

                let overlap_min = min_x_a.max(min_x_b);
                let overlap_max = max_x_a.min(max_x_b);
                if overlap_max > overlap_min {
                    parallel_length_nm = overlap_max - overlap_min;
                }
            } else if is_a_vertical && is_b_vertical {
                let center_dist_x = (seg_a.start_x - seg_b.start_x).abs();
                edge_to_edge_spacing_nm = center_dist_x - (seg_a.width_nm + seg_b.width_nm) / 2.0;

                let min_y_a = seg_a.start_y.min(seg_a.end_y);
                let max_y_a = seg_a.start_y.max(seg_a.end_y);
                let min_y_b = seg_b.start_y.min(seg_b.end_y);
                let max_y_b = seg_b.start_y.max(seg_b.end_y);

                let overlap_min = min_y_a.max(min_y_b);
                let overlap_max = max_y_a.min(max_y_b);
                if overlap_max > overlap_min {
                    parallel_length_nm = overlap_max - overlap_min;
                }
            }

            if parallel_length_nm > 0.0
                && edge_to_edge_spacing_nm > 0.0
                && edge_to_edge_spacing_nm <= config.max_coupling_distance_nm
            {
                let epsilon_r = 3.9;
                let area_m2 = (parallel_length_nm * 1e-9) * (seg_a.thickness_nm * 1e-9);
                let spacing_m = edge_to_edge_spacing_nm * 1e-9;
                let c_parallel_plate = EPS_0 * epsilon_r * (area_m2 / spacing_m);
                let c_fringe = 0.5 * EPS_0 * epsilon_r * (parallel_length_nm * 1e-9);
                let total_coupling_f = c_parallel_plate + c_fringe;

                if total_coupling_f > 1e-17 {
                    let cap_name = format!(
                        "Cc_{}_{}_{}_{}",
                        seg_a.net_name, seg_b.net_name, i, j
                    );
                    graph.parasitics.push(ParasiticElement::CouplingCapacitance {
                        name: cap_name,
                        node_a: seg_a.node.clone(),
                        node_b: seg_b.node.clone(),
                        value_farads: total_coupling_f,
                    });
                }
            }
        }
    }
}
