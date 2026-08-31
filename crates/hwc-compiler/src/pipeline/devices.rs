use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use hwc_types::NetId;
use rustc_hash::FxHashMap;

use crate::eval::MemoryEmitter;
use crate::pipeline::error::PipelineError;
use crate::symbol_table::SymbolTable;

/// Populate device instances from emitted primitives.
pub fn populate_devices(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
    net_id_to_name: &FxHashMap<NetId, CompactString>,
    symbol_table: &SymbolTable,
) -> Result<(), PipelineError> {
    let mut port_layers: FxHashMap<CompactString, CompactString> = FxHashMap::default();
    for route in &mem.routes {
        if let crate::eval::Value::PlacedPort(p) = &route.from {
            port_layers.insert(p.port_name.clone(), p.layer.clone());
        }
        if let crate::eval::Value::PlacedPort(p) = &route.to {
            port_layers.insert(p.port_name.clone(), p.layer.clone());
        }
    }
    for poly in &mem.polygons {
        if let Some(ref p) = poly.port {
            port_layers.insert(p.clone(), poly.layer.clone());
        }
    }

    for dev in &mem.devices {
        // Validate device contract existence in SymbolTable immediately at space compilation time
        let device_decl = symbol_table
            .get_device(&dev.device_type)
            .map_err(|_| PipelineError {
                message: format!(
                    "Device instance '{}' uses device type '{}', but no definition for 'device {}' was found in the SymbolTable. Ensure it is declared and exported/imported.",
                    dev.name, dev.device_type, dev.device_type
                ),
            })?;

        let spice_meta = device_decl.spice();
        let expected_terminals = if !spice_meta.terminal_order.is_empty() {
            spice_meta.terminal_order
        } else {
            let mut terms = Vec::new();
            if let Some(sec) = device_decl.get_section("terminals") {
                for (_, expr) in &sec.fields {
                    if let hwc_parser::ast::Expression::ArrayLiteral { elements, .. } = expr {
                        for elem in elements {
                            if let hwc_parser::ast::Expression::Variable { name, .. } = elem {
                                terms.push(name.clone());
                            }
                        }
                    }
                }
            }
            terms
        };

        for exp_term in &expected_terminals {
            if !dev.terminals.contains_key(exp_term) && !dev.terminal_ports.contains_key(exp_term) {
                return Err(PipelineError {
                    message: format!(
                        "Device instance '{}' (type '{}') is missing connection for required terminal '{}'.",
                        dev.name, dev.device_type, exp_term
                    ),
                });
            }
        }

        let mut terms = Vec::new();
        let mut term_nets = FxHashMap::default();
        for (term_name, net_id) in &dev.terminals {
            terms.push(term_name.clone());
            if let Some(resolved_net) = net_id_to_name.get(net_id) {
                term_nets.insert(term_name.clone(), resolved_net.clone());
            }
        }

        // Strongly-typed terminal net resolution from routes connecting to PlacedPort endpoints
        for route in &mem.routes {
            let route_net = if let Some(crate::eval::Value::NetHandle(id)) = route.properties.get("net") {
                net_id_to_name.get(id).cloned()
            } else {
                None
            };

            if let Some(net_name) = route_net {
                if let crate::eval::Value::PlacedPort(p) = &route.from {
                    if dev.name == p.instance_name || dev.name == p.cell_name {
                        term_nets.insert(p.port_name.clone(), net_name.clone());
                    }
                }
                if let crate::eval::Value::PlacedPort(p) = &route.to {
                    if dev.name == p.instance_name || dev.name == p.cell_name {
                        term_nets.insert(p.port_name.clone(), net_name.clone());
                    }
                }
            }
        }

        // Map declared terminal names (e.g. C0 -> TOP, C1 -> BOT) to the net connected to that port
        for (term_name, port_target) in &dev.terminal_ports {
            if let Some(net) = term_nets.get(port_target) {
                term_nets.insert(term_name.clone(), net.clone());
            }
        }

        let mut term_ports = FxHashMap::default();
        let mut term_layers = FxHashMap::default();
        let mut term_bindings = Vec::new();

        for (term_name, port_target) in &dev.terminal_ports {
            term_ports.insert(term_name.clone(), port_target.clone());
            if let Some(layer) = port_layers.get(port_target) {
                term_layers.insert(term_name.clone(), layer.clone());
                let layer_id = hw_space
                    .stackup_layers
                    .iter()
                    .position(|l| l.name == *layer)
                    .map(|idx| hwc_types::LayerId::new(idx as u16))
                    .unwrap_or(hwc_types::LayerId::new(0));
                let net_name = term_nets.get(term_name).cloned().unwrap_or_default();
                let net_id = mem.nets.get(&net_name).copied().unwrap_or(hwc_types::NetId::UNCONNECTED);

                term_bindings.push(hwc_types::DeviceTerminalBinding {
                    instance_name: dev.name.clone(),
                    terminal: term_name.clone(),
                    port: port_target.clone(),
                    layer_id,
                    layer_name: layer.clone(),
                    net_id,
                    net_name,
                });
            }
        }

        let mut port_positions = FxHashMap::default();
        for route in &mem.routes {
            if let crate::eval::Value::PlacedPort(p) = &route.from {
                if dev.name == p.instance_name || dev.name == p.cell_name {
                    port_positions.insert(p.port_name.clone(), (p.world_x / 1000, p.world_y / 1000));
                }
            }
            if let crate::eval::Value::PlacedPort(p) = &route.to {
                if dev.name == p.instance_name || dev.name == p.cell_name {
                    port_positions.insert(p.port_name.clone(), (p.world_x / 1000, p.world_y / 1000));
                }
            }
        }

        let mut params_map = FxHashMap::default();
        for (p_name, m_val) in &dev.params {
            params_map.insert(p_name.clone(), (m_val.raw as f64) * 1e-12);
        }

        hw_space.device_instances.push(hwc_engine::space::DeviceInstance {
            name: dev.name.clone(),
            def_path: None,
            device_type: dev.device_type.clone(),
            terminals: terms,
            terminal_nets: term_nets,
            terminal_ports: term_ports,
            terminal_layers: term_layers,
            terminal_bindings: term_bindings,
            parameters: params_map,
            port_positions,
        });
    }

    Ok(())
}
