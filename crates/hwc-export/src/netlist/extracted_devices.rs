//! Emits SPICE cards for extracted physical devices and integrated trace parasitics.

use hwc_compiler::eval::{MeasurementValue, UnitDimension};
use hwc_compiler::SymbolTable;

use super::types::{PhysicalNetlist, PhysicalNetlistGraph};

fn format_measurement_spice_unit(m: &MeasurementValue) -> String {
    if m.dimension == UnitDimension::LENGTH {
        let um = (m.raw as f64) * 1e-6;
        format!("{:.2}u", um)
    } else if m.dimension == UnitDimension::RESISTANCE {
        let ohms = (m.raw as f64) * 1e-6;
        format!("{:.2}", ohms)
    } else if m.dimension == UnitDimension::CAPACITANCE {
        let pf = (m.raw as f64) * 1e-6;
        format!("{:.2}p", pf)
    } else if m.dimension == UnitDimension::INDUCTANCE {
        let nh = (m.raw as f64) * 1e-3;
        format!("{:.2}n", nh)
    } else if m.dimension == UnitDimension::VOLTAGE {
        let v = (m.raw as f64) * 1e-9;
        format!("{:.2}V", v)
    } else if m.dimension == UnitDimension::CURRENT {
        let ua = (m.raw as f64) * 1e-6;
        format!("{:.2}uA", ua)
    } else {
        format!("{}", m.raw)
    }
}

/// Emit EXTRACTED DEVICES into SPICE netlist.
pub fn emit_extracted_devices(
    netlist_str: &mut String,
    netlist: &PhysicalNetlist,
    symbol_table: &SymbolTable,
    physical_graph: &PhysicalNetlistGraph,
) -> Result<(), Box<dyn std::error::Error>> {
    if netlist.devices.is_empty() {
        return Ok(());
    }

    netlist_str.push_str("\n* ========================================\n");
    netlist_str.push_str("* EXTRACTED DEVICES\n");
    netlist_str.push_str("* ========================================\n");

    for device in &netlist.devices {
        // 1. Fetch user's device contract directly from SymbolTable AST
        let device_decl = symbol_table
            .get_device(&device.device_type)
            .map_err(|_| format!("FATAL: Device definition '{}' not found in SymbolTable", device.device_type))?;

        // 2. Extract user-declared SPICE metadata (Zero hardcoded strings!)
        let spice_decl = device_decl.spice();
        let prefix = spice_decl.prefix.as_deref().unwrap_or("X");
        let subcircuit = spice_decl.subcircuit.as_deref().unwrap_or(&device.device_type);
        let mut terminal_order = spice_decl.terminal_order;
        if terminal_order.is_empty() {
            terminal_order = device.terminals.keys().cloned().collect();
        }
        let param_order = spice_decl.parameters;
        let param_style = spice_decl.parameter_style.as_deref().unwrap_or("named");

        // 3. Resolve EXACTLY the nodes in terminal_order (No extra port keys!)
        let mut resolved_nodes = Vec::with_capacity(terminal_order.len());
        for term_name in &terminal_order {
            let node = physical_graph
                .device_nodes
                .get(&(device.name.to_string(), term_name.to_string()))
                .or_else(|| {
                    if let Some(target_port) = device.terminal_ports.get(term_name) {
                        physical_graph.device_nodes.get(&(device.name.to_string(), target_port.to_string()))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    format!(
                        "FATAL: Device '{}' (type '{}') missing connection for terminal '{}'",
                        device.name, device.device_type, term_name
                    )
                })?;
            resolved_nodes.push(node.as_str());
        }

        // 4. Format parameters
        let mut params_str = String::new();
        if !param_order.is_empty() {
            for param_name in &param_order {
                if let Some(val) = device.params.get(param_name)
                    .or_else(|| device.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(param_name)).map(|(_, v)| v))
                {
                    let formatted = format_measurement_spice_unit(val);
                    if param_style == "named" {
                        params_str.push_str(&format!(" {}={}", param_name.to_lowercase(), formatted));
                    } else {
                        params_str.push_str(&format!(" {}", formatted));
                    }
                }
            }
        } else {
            for (p, val) in &device.params {
                params_str.push_str(&format!(" {}={}", p.to_lowercase(), format_measurement_spice_unit(val)));
            }
        }

        // 5. Emit clean SPICE line (Matches exact terminal count!)
        netlist_str.push_str(&format!(
            "{}{} {} {}{}\n",
            prefix,
            device.name,
            resolved_nodes.join(" "),
            subcircuit,
            params_str
        ));
    }
    netlist_str.push('\n');

    Ok(())
}

/// Emit integrated trace parasitics (R, C) into the netlist body.
pub fn emit_parasitics(netlist_str: &mut String, physical_graph: &PhysicalNetlistGraph) {
    if physical_graph.parasitics.is_empty() {
        return;
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* INTEGRATED TRACE PARASITICS\n");
    netlist_str.push_str("* ========================================\n");

    for parasitic in &physical_graph.parasitics {
        match parasitic {
            super::types::ParasiticElement::TraceResistor {
                name,
                node_a,
                node_b,
                value_ohms,
            } => {
                let (prefix, comment) = if name.starts_with("via_") || name.starts_with("Rvia_") {
                    ("R", "* Via/Contact resistance\n")
                } else {
                    ("", "* Trace resistance\n")
                };
                netlist_str.push_str(comment);
                let card_name = if name.starts_with('R') {
                    name.clone()
                } else {
                    format!("{}{}", prefix, name)
                };
                netlist_str.push_str(&format!(
                    "{} {} {} {:.6e}\n",
                    card_name, node_a, node_b, value_ohms
                ));
            }
            super::types::ParasiticElement::GroundCapacitance {
                name,
                node,
                ref_node,
                value_farads,
            } => {
                netlist_str.push_str("* Ground capacitance\n");
                let card_name = if name.starts_with('C') {
                    name.clone()
                } else {
                    format!("C{}", name)
                };
                netlist_str.push_str(&format!(
                    "{} {} {} {:.6e}\n",
                    card_name, node, ref_node, value_farads
                ));
            }
            super::types::ParasiticElement::CouplingCapacitance {
                name,
                node_a,
                node_b,
                value_farads,
            } => {
                netlist_str.push_str("* Lateral coupling capacitance\n");
                let card_name = if name.starts_with('C') {
                    name.clone()
                } else {
                    format!("C{}", name)
                };
                netlist_str.push_str(&format!(
                    "{} {} {} {:.6e}\n",
                    card_name, node_a, node_b, value_farads
                ));
            }
        }
    }

    netlist_str.push_str(&format!(
        "\n* Total parasitic elements: {}\n",
        physical_graph.parasitics.len()
    ));
    netlist_str.push('\n');
}
