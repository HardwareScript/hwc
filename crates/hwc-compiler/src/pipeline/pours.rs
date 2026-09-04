//! Pour population for the v0.3.0 pipeline.
//!
//! Lowers every emitted polygon primitive into a [`hwc_engine::PourMetadata`]
//! record and an EntityGraph `SubstrateLayer` (pour), generating a stable
//! semantic name when the emitter did not supply one.

use compact_str::CompactString;
use hwc_engine::space::{BindingPriority, DeviceBinding};
use hwc_engine::HardwareSpace;
use hwc_types::NetId;
use rustc_hash::FxHashMap;

use crate::eval::MemoryEmitter;
use crate::pipeline::error::PipelineError;

/// Populate pours from emitted polygons and inject them into the space's EntityGraph.
pub fn populate_pours(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
    net_id_to_name: &FxHashMap<NetId, CompactString>,
    profile_name: &str,
) -> Result<(), PipelineError> {
    // 3. Populate pours from polygons & inject into EntityGraph
    let mut pour_counters: FxHashMap<(CompactString, Option<CompactString>), usize> = FxHashMap::default();

    // Precompute port -> net mapping from routed endpoints with spatial coordinates
    let mut port_coord_net_map: Vec<((i64, i64), CompactString, CompactString)> = Vec::new();

    for route in &mem.routes {
        let route_net_name = if let Some(crate::eval::Value::NetHandle(id)) = route.properties.get("net") {
            net_id_to_name.get(id).cloned()
        } else {
            None
        };

        if let Some(net_name) = route_net_name {
            if let crate::eval::Value::PlacedPort(p) = &route.from {
                port_coord_net_map.push(((p.world_x / 1000, p.world_y / 1000), p.port_name.clone(), net_name.clone()));
            }
            if let crate::eval::Value::PlacedPort(p) = &route.to {
                port_coord_net_map.push(((p.world_x / 1000, p.world_y / 1000), p.port_name.clone(), net_name.clone()));
            }
        }
    }

    for (idx, poly) in mem.polygons.iter().enumerate() {
                
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        for (x_pm, y_pm) in &poly.points {
            let x_nm = x_pm / 1000;
            let y_nm = y_pm / 1000;
            min_x = min_x.min(x_nm);
            min_y = min_y.min(y_nm);
            max_x = max_x.max(x_nm);
            max_y = max_y.max(y_nm);
        }

        // Resolve layer Z elevations and material
        let st_pos = hw_space
            .stackup_layers
            .iter()
            .position(|l| l.name == poly.layer);
        let st_idx = st_pos.ok_or_else(|| PipelineError {
            message: format!(
                "Polygon '{}' references layer '{}' which is not defined in profile '{}'. Available layers: {}",
                poly.semantic_name.as_deref().unwrap_or("unnamed"),
                poly.layer,
                profile_name,
                hw_space
                    .stackup_layers
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })?;
        let st = &hw_space.stackup_layers[st_idx];
        let stackup_layer_id = hwc_types::LayerId::new(st_idx as u16);
        let (z_bottom, z_top, mat_name) = (st.z_bottom, st.z_top, st.material_name.clone());

        let mat_id = hw_space
            .material_registry
            .get_id(&mat_name)
            .ok_or_else(|| PipelineError {
                message: format!(
                    "Polygon '{}' on layer '{}' references material '{}' which is not defined. Available materials: {}",
                    poly.semantic_name.as_deref().unwrap_or("unnamed"),
                    poly.layer,
                    mat_name,
                    hw_space
                        .material_registry
                        .all_materials()
                        .iter()
                        .map(|(_, name)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;

        let bbox = if min_x <= max_x && min_y <= max_y {
            Some(hwc_engine::BoundingBox::new(
                hwc_engine::Point3D::new(min_x, min_y, z_bottom),
                hwc_engine::Point3D::new(max_x, max_y, z_top),
            ))
        } else {
            None
        };

        let mut net_name = poly.net.and_then(|id| net_id_to_name.get(&id).cloned());

        // 1. If polygon has an explicit port tag (e.g. port: "PORT" or port: "BULK"),
        // match against routed ports with that name whose coordinate falls inside or touches the bbox
        if net_name.is_none() {
            if let Some(ref port_name) = poly.port {
                for ((px, py), p_name, n) in &port_coord_net_map {
                    if p_name == port_name && *px >= min_x && *px <= max_x && *py >= min_y && *py <= max_y {
                        net_name = Some(n.clone());
                        break;
                    }
                }
            }
        }

        // 2. Spatial proximity fallback: any routed port coordinate touching polygon bbox
        if net_name.is_none() {
            for ((px, py), _p_name, n) in &port_coord_net_map {
                if *px >= min_x && *px <= max_x && *py >= min_y && *py <= max_y {
                    net_name = Some(n.clone());
                    break;
                }
            }
        }

        let w = (max_x - min_x).max(0);
        let h = (max_y - min_y).max(0);

        let engine_net = if let Some(ref n) = net_name {
            if let Some(id) = mem.nets.get(n) {
                hwc_engine::netlist::NetId::new(id.0)
            } else {
                hw_space.netlist.get_or_create_net(n.as_str())
            }
        } else {
            hwc_engine::netlist::NetId::UNCONNECTED
        };

        if let Some(b) = bbox {
            let substrate_layer = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                mat_id,
                engine_net,
                b,
                hwc_physics::connectivity::SubstrateLayerType::Pour,
            )
            .with_layer_name(poly.layer.clone())
            .with_layer_id(stackup_layer_id);
            hw_space.entity_graph.substrate_layers.push(substrate_layer);
        }

        // v0.3.0: Generate semantic pour names based on net and layer
        // Format: <Cell>_<Layer> or <Net>_<Layer> to ensure each polygon within a macro is attributed to its own layer
        let pour_name = if let Some(semantic_name) = &poly.semantic_name {
            if semantic_name.ends_with(poly.layer.as_str()) {
                semantic_name.clone()
            } else {
                CompactString::new(format!("{}_{}", semantic_name, poly.layer))
            }
        } else {
            // Generate semantic name: NetName_Layer or just Layer_N if no net
            let counter_key = (poly.layer.clone(), net_name.clone());
            let counter = pour_counters.entry(counter_key).or_insert(0);
            *counter += 1;

            if let Some(ref net) = net_name {
                if *counter == 1 {
                    // First pour on this net+layer: use simpler name
                    CompactString::new(format!("{}_{}", net, poly.layer))
                } else {
                    // Multiple pours: add counter
                    CompactString::new(format!("{}_{}_{}", net, poly.layer, *counter - 1))
                }
            } else {
                // No net: use layer name with counter
                CompactString::new(format!("{}_{}", poly.layer, idx))
            }
        };

        if net_name.is_some() {
            let comp_id = hw_space
                .netlist
                .add_component(pour_name.clone(), poly.layer.clone(), (min_x, min_y, z_bottom));
            let pin_anchor = hw_space
                .netlist
                .add_pin(comp_id, "anchor".into(), (0, 0, 0), None);
            hw_space.netlist.connect_pin(pin_anchor, engine_net);
            let pin_virt = hw_space.netlist.add_pin(
                comp_id,
                format!("__virtual_{}", pour_name).into(),
                (0, 0, 0),
                None,
            );
            hw_space.netlist.connect_pin(pin_virt, engine_net);
        }

        // Bind pour to device if semantic_name matches device and pour has no direct net
        let dev_binding = if net_name.is_none() {
            if let Some(semantic_name) = &poly.semantic_name {
                mem.devices.iter().find(|d| &d.name == semantic_name || &d.device_type == semantic_name).map(|d| {
                    DeviceBinding {
                        device_name: d.name.clone(),
                        terminals: d.terminals.keys().cloned().collect(),
                        priority: BindingPriority::Channel,
                        def_path: None,
                    }
                })
            } else {
                None
            }
        } else {
            None
        };

        hw_space.pours.push(hwc_engine::PourMetadata {
            name: pour_name.clone(),
            material_name: mat_name.clone(),
            layer_name: poly.layer.clone(),
            layer_id: Some(stackup_layer_id),
            z_bottom_nm: z_bottom,
            net: net_name.clone(),
            area_nm2: w * h,
            bbox,
            device_binding: dev_binding,
            merged_region_id: None,
            via_landing_nodes: Vec::new(),
            waivers: hwc_parser::Waivers::default(),
        });
    }

    Ok(())
}
