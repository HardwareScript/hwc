//! Route population for the v0.3.0 pipeline.
//!
//! Lowers every emitted route into an [`hwc_engine::space::AnalyticTrace`] and an
//! EntityGraph `SubstrateLayer` (pour). The layer and material are resolved from
//! the space's stackup; missing layers/materials produce a descriptive error.

use compact_str::CompactString;
use hwc_engine::HardwareSpace;

use crate::eval::{MemoryEmitter, Value};
use crate::pipeline::error::PipelineError;

/// Populate analytic routes from emitted primitives and inject them into the space.
pub fn populate_routes(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
) -> Result<(), PipelineError> {
    // 6. Populate routes into analytic_routes and entity_graph
    for route in &mem.routes {
        if let (Ok(p1), Ok(p2)) = (route.from.coerce_to_point2d(), route.to.coerce_to_point2d()) {
            if let (Value::Point2D { x: x1, y: y1 }, Value::Point2D { x: x2, y: y2 }) = (p1, p2) {
                // Use port center coordinates for physical routing to ensure PIVB connectivity
                let pt1_nm = (x1 / 1000, y1 / 1000);
                let pt2_nm = (x2 / 1000, y2 / 1000);
                let mut resolved_net_name: Option<CompactString> = None;
                let mut resolved_net_id = hwc_engine::netlist::NetId::UNCONNECTED;

                for pour in &hw_space.pours {
                    if let (Some(ref bbox), Some(ref net_name)) = (&pour.bbox, &pour.net) {
                        if (pt1_nm.0 >= bbox.min.x
                            && pt1_nm.0 <= bbox.max.x
                            && pt1_nm.1 >= bbox.min.y
                            && pt1_nm.1 <= bbox.max.y)
                            || (pt2_nm.0 >= bbox.min.x
                                && pt2_nm.0 <= bbox.max.x
                                && pt2_nm.1 >= bbox.min.y
                                && pt2_nm.1 <= bbox.max.y)
                        {
                            resolved_net_name = Some(net_name.clone());
                            if let Some(id) = mem.nets.get(net_name) {
                                resolved_net_id = hwc_engine::netlist::NetId::new(id.0);
                            }
                            break;
                        }
                    }
                }

                // v0.3.0 FIX: Extract layer from route properties, default to metal1
                let layer_name = route
                    .properties
                    .get("layer")
                    .and_then(|v| match v {
                        Value::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("metal1");

                let net_label = resolved_net_name.clone().unwrap_or_else(|| "ROUTE".into());

                let layer_st = hw_space.stackup_layers.iter().find(|l| l.name == layer_name);
                let z_min = layer_st.map(|l| l.z_bottom).unwrap_or(630);
                let z_max = layer_st.map(|l| l.z_top).unwrap_or(990);
                let z_center = (z_min + z_max) / 2;

                // v0.3.0 FIX: Use material from the routing layer's stackup definition
                // NO FALLBACK - fail if layer or material is missing
                let trace_mat_name = layer_st
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Route on net '{}' references unknown layer '{}'. Available layers: {}",
                            net_label,
                            layer_name,
                            hw_space
                                .stackup_layers
                                .iter()
                                .map(|l| l.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    })?
                    .material_name
                    .clone();

                let trace_mat_id = hw_space
                    .material_registry
                    .get_id(&trace_mat_name)
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Route on layer '{}' requires material '{}' which is not registered. Define this material in your .hw file.",
                            layer_name, trace_mat_name
                        ),
                    })?;

                // Trace cross-section width (standard metal1 interconnect)
                let trace_width_nm = 300i64;

                let trace_params = hwc_engine::space::AnalyticTraceParams {
                    net_id: resolved_net_id,
                    cross_section: hwc_engine::space::CrossSection::new(
                        trace_width_nm,
                        (z_max - z_min).max(100),
                    ),
                    segments: vec![hwc_engine::space::LineSegment::new(
                        hwc_engine::Point3D::new(pt1_nm.0, pt1_nm.1, z_center),
                        hwc_engine::Point3D::new(pt2_nm.0, pt2_nm.1, z_center),
                    )],
                    material: trace_mat_id,
                    net_name: net_label,
                    current: hwc_engine::space::CurrentRating::new(0.0, 0.0),
                    layer_z_range: Some((z_min, z_max)),
                    layer_name: layer_name.into(),
                };
                hw_space
                    .analytic_routes
                    .push(hwc_engine::space::AnalyticTrace::with_layer_z_range(trace_params));

                // Physical polygon bounding box for Clipper2 welding & PIVB connectivity
                // Must span center-to-center to overlap with pad/contact pours
                let half_w = trace_width_nm / 2;
                let trace_min_x = pt1_nm.0.min(pt2_nm.0) - half_w;
                let trace_max_x = pt1_nm.0.max(pt2_nm.0) + half_w;
                let trace_min_y = pt1_nm.1.min(pt2_nm.1) - half_w;
                let trace_max_y = pt1_nm.1.max(pt2_nm.1) + half_w;
                let trace_bbox = hwc_engine::BoundingBox::new(
                    hwc_engine::Point3D::new(trace_min_x, trace_min_y, z_min),
                    hwc_engine::Point3D::new(trace_max_x, trace_max_y, z_max),
                );
                let substrate_trace = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                    trace_mat_id,
                    resolved_net_id,
                    trace_bbox,
                    hwc_physics::connectivity::SubstrateLayerType::Pour,
                );
                hw_space.entity_graph.substrate_layers.push(substrate_trace);
            }
        }
    }

    Ok(())
}
