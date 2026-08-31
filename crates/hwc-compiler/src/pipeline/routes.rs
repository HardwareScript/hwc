//! Route population for the v0.3.0 pipeline.
//!
//! Lowers every emitted route into an [`hwc_engine::space::AnalyticTrace`] and an
//! EntityGraph `SubstrateLayer` (pour) using deterministic type and property resolution.
//! Zero fallback to phantom nets, zero layer poisoning.

use compact_str::CompactString;
use hwc_engine::HardwareSpace;

use crate::eval::{MemoryEmitter, Value};
use crate::pipeline::error::PipelineError;

/// Populate analytic routes from emitted primitives and inject them into the space.
pub fn populate_routes(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
) -> Result<(), PipelineError> {
    for route in &mem.routes {
        lower_route_or_bundle(hw_space, mem, route)?;
    }
    Ok(())
}

fn lower_route_or_bundle(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
    route: &crate::eval::RouteRecord,
) -> Result<(), PipelineError> {
    // 1. Check if both endpoints are structural bundle structs (e.g. diff pairs, buses)
    if let (Value::StructInstance { fields: from_fields, .. }, Value::StructInstance { fields: to_fields, .. }) = (&route.from, &route.to) {
        if route.from.coerce_to_point2d().is_err() || route.to.coerce_to_point2d().is_err() {
            let mut matched = false;
            for (from_k, from_v) in from_fields.iter() {
                if let Some((_, to_v)) = to_fields.iter().find(|(to_k, _)| to_k == from_k) {
                    matched = true;
                    let sub_route = crate::eval::RouteRecord {
                        space_id: route.space_id,
                        from: from_v.clone(),
                        to: to_v.clone(),
                        intent: route.intent.clone(),
                        properties: route.properties.clone(),
                    };
                    lower_route_or_bundle(hw_space, mem, &sub_route)?;
                }
            }
            if matched {
                return Ok(());
            }
        }
    }

    lower_single_route(hw_space, mem, route)
}

fn lower_single_route(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
    route: &crate::eval::RouteRecord,
) -> Result<(), PipelineError> {
    // 1. Extract endpoint coordinates (coerced deterministically to Point2D)
    let (pt1_nm, pt2_nm) = match (route.from.coerce_to_point2d(), route.to.coerce_to_point2d()) {
        (Ok(Value::Point2D { x: x1, y: y1 }), Ok(Value::Point2D { x: x2, y: y2 })) => {
            ((x1 / 1000, y1 / 1000), (x2 / 1000, y2 / 1000))
        }
        _ => {
            return Err(PipelineError {
                message: "Route endpoints must evaluate or coerce to Point2D".to_string(),
            })
        }
    };

    // 2. Resolve Net and Layer from Pours at Endpoints
    let mut resolved_net_name: Option<CompactString> = None;
    let mut resolved_net_id: Option<hwc_engine::netlist::NetId> = None;
    let mut resolved_layer_name: Option<CompactString> = None;

    // Collect each endpoint's declared layer for continuity validation
    let from_port_layer: Option<CompactString> = match &route.from {
        Value::PlacedPort(p) => Some(p.layer.clone()),
        _ => None,
    };
    let to_port_layer: Option<CompactString> = match &route.to {
        Value::PlacedPort(p) => Some(p.layer.clone()),
        _ => None,
    };

    // Check explicit route properties first
    if let Some(Value::String(s)) = route.properties.get("layer") {
        resolved_layer_name = Some(s.clone());
    }
    if let Some(Value::NetHandle(id)) = route.properties.get("net") {
        resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
    }

    // Check if endpoints are typed PlacedPort structs with `layer` or `net`
    if let Value::PlacedPort(p) = &route.from {
        if resolved_layer_name.is_none() {
            resolved_layer_name = Some(p.layer.clone());
        }
        if resolved_net_id.is_none() {
            if let Some(id) = p.net {
                resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
            }
        }
    }
    if let Value::PlacedPort(p) = &route.to {
        if resolved_layer_name.is_none() {
            resolved_layer_name = Some(p.layer.clone());
        }
        if resolved_net_id.is_none() {
            if let Some(id) = p.net {
                resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
            }
        }
    }
    if let Value::StructInstance { fields, .. } = &route.from {
        if resolved_layer_name.is_none() {
            if let Some((_, Value::String(s))) = fields.iter().find(|(k, _)| k.as_str() == "layer") {
                resolved_layer_name = Some(s.clone());
            }
        }
        if resolved_net_id.is_none() {
            if let Some((_, Value::NetHandle(id))) = fields.iter().find(|(k, _)| k.as_str() == "net") {
                resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
            }
        }
    }
    if let Value::StructInstance { fields, .. } = &route.to {
        if resolved_layer_name.is_none() {
            if let Some((_, Value::String(s))) = fields.iter().find(|(k, _)| k.as_str() == "layer") {
                resolved_layer_name = Some(s.clone());
            }
        }
        if resolved_net_id.is_none() {
            if let Some((_, Value::NetHandle(id))) = fields.iter().find(|(k, _)| k.as_str() == "net") {
                resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
            }
        }
    }

    // If not explicit, query the physical pours at the endpoints (Spatial Truth)
    if resolved_net_id.is_none() || resolved_layer_name.is_none() {
        // Find all routable conductor pours overlapping pt1 or pt2
        for pour in &hw_space.pours {
            if let Some(ref bbox) = &pour.bbox {
                let st = hw_space.stackup_layers.iter().find(|l| l.name == pour.layer_name);
                let is_routable = st.map_or(false, |l| l.is_routable);
                if !is_routable {
                    continue;
                }

                let touches_pt1 = pt1_nm.0 >= bbox.min.x && pt1_nm.0 <= bbox.max.x && pt1_nm.1 >= bbox.min.y && pt1_nm.1 <= bbox.max.y;
                let touches_pt2 = pt2_nm.0 >= bbox.min.x && pt2_nm.0 <= bbox.max.x && pt2_nm.1 >= bbox.min.y && pt2_nm.1 <= bbox.max.y;

                if touches_pt1 || touches_pt2 {
                    if resolved_layer_name.is_none() {
                        resolved_layer_name = Some(pour.layer_name.clone());
                    }
                    if resolved_net_id.is_none() {
                        if let Some(ref net) = pour.net {
                            resolved_net_name = Some(net.clone());
                            if let Some(id) = mem.nets.get(net) {
                                resolved_net_id = Some(hwc_engine::netlist::NetId::new(id.0));
                            }
                        }
                    }
                }
            }
        }
    }

    let layer_name = resolved_layer_name.ok_or_else(|| PipelineError {
        message: format!(
            "Route between ({}, {}) and ({}, {}) cannot determine routing layer. Specify 'layer: \"...\"' on the route.",
            pt1_nm.0, pt1_nm.1, pt2_nm.0, pt2_nm.1
        ),
    })?;

    let engine_net_id = resolved_net_id.ok_or_else(|| PipelineError {
        message: format!(
            "Route between ({}, {}) and ({}, {}) on layer '{}' does not touch any known net pour. Specify 'net: NetName' on the route.",
            pt1_nm.0, pt1_nm.1, pt2_nm.0, pt2_nm.1, layer_name
        ),
    })?;

    let net_name = resolved_net_name.unwrap_or_else(|| {
        for (name, id) in &mem.nets {
            if id.0 == engine_net_id.raw() {
                return name.clone();
            }
        }
        CompactString::new(format!("NET_{}", engine_net_id.raw()))
    });

    let layer_st = hw_space
        .stackup_layers
        .iter()
        .find(|l| l.name == layer_name)
        .ok_or_else(|| PipelineError {
            message: format!(
                "Route references layer '{}' which is not defined in profile stackup. Available: {}",
                layer_name,
                hw_space.stackup_layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        })?;

    // ── [DRC] Route Layer Continuity ─────────────────────────────────────────
    // When both endpoints are typed PlacedPorts, each port's physical layer
    // must match the resolved route layer. Routing metal1 → metal3 without
    // explicit vias is a physical open circuit.
    if let Some(ref from_layer) = from_port_layer {
        if from_layer.as_str() != layer_name.as_str() {
            return Err(PipelineError {
                message: format!(
                    "[DRC] Route layer continuity violation: 'from' port is on layer '{}' but route resolves to layer '{}'. \
                     Add 'layer: \"{}\"' explicitly on the route, or add via cells to bridge the layers.",
                    from_layer, layer_name, from_layer
                ),
            });
        }
    }
    if let Some(ref to_layer) = to_port_layer {
        if to_layer.as_str() != layer_name.as_str() {
            return Err(PipelineError {
                message: format!(
                    "[DRC] Route layer continuity violation: 'to' port is on layer '{}' but route resolves to layer '{}'. \
                     Add 'layer: \"{}\"' explicitly on the route, or add via cells to bridge the layers.",
                    to_layer, layer_name, to_layer
                ),
            });
        }
    }

    let z_min = layer_st.z_bottom;
    let z_max = layer_st.z_top;
    let z_center = (z_min + z_max) / 2;

    let trace_mat_name = layer_st.material_name.clone();
    let trace_mat_id = hw_space
        .material_registry
        .get_id(&trace_mat_name)
        .ok_or_else(|| PipelineError {
            message: format!("Material '{}' not registered", trace_mat_name),
        })?;

    // Resolve trace width from explicit route property, else fall back to 300 nm minimum.
    let trace_width_nm: i64 = if let Some(Value::Measurement(m)) = route.properties.get("width") {
        // Measurement is stored in picometres; 1 nm = 1000 pm
        ((m.raw / 1000) as i64).max(1)
    } else {
        300
    };
    let trace_params = hwc_engine::space::AnalyticTraceParams {
        net_id: engine_net_id,
        cross_section: hwc_engine::space::CrossSection::new(
            trace_width_nm,
            (z_max - z_min).max(100),
        ),
        segments: vec![hwc_engine::space::LineSegment::new(
            hwc_engine::Point3D::new(pt1_nm.0, pt1_nm.1, z_center),
            hwc_engine::Point3D::new(pt2_nm.0, pt2_nm.1, z_center),
        )],
        material: trace_mat_id,
        net_name: net_name.clone(),
        current: hwc_engine::space::CurrentRating::new(0.0, 0.0),
        layer_z_range: Some((z_min, z_max)),
        layer_name: layer_name.into(),
    };

    hw_space
        .analytic_routes
        .push(hwc_engine::space::AnalyticTrace::with_layer_z_range(trace_params));

    Ok(())
}
