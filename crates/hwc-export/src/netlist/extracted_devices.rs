//! Emits SPICE cards for extracted physical devices and integrated trace parasitics.

use compact_str::CompactString;
use hwc_compiler::eval::{MeasurementValue, UnitDimension};
use hwc_compiler::SymbolTable;

use super::types::{PhysicalNetlist, PhysicalNetlistGraph};

fn format_measurement_spice(m: &MeasurementValue) -> String {
    match m.dimension {
        UnitDimension::Length => {
            let meters = (m.raw as f64) * 1e-12;
            format!("{:.6e}", meters)
        }
        UnitDimension::Resistance => {
            let ohms = (m.raw as f64) * 1e-6;
            format!("{:.6e}", ohms)
        }
        UnitDimension::Capacitance => {
            let farads = (m.raw as f64) * 1e-18;
            format!("{:.6e}", farads)
        }
        UnitDimension::Inductance => {
            let henries = (m.raw as f64) * 1e-12;
            format!("{:.6e}", henries)
        }
        UnitDimension::Voltage => {
            let volts = (m.raw as f64) * 1e-9;
            format!("{:.6e}", volts)
        }
        UnitDimension::Current => {
            let amps = (m.raw as f64) * 1e-12;
            format!("{:.6e}", amps)
        }
        _ => format!("{}", m.raw),
    }
}

/// Emit EXTRACTED DEVICES into SPICE netlist.
pub fn emit_extracted_devices(
    netlist_str: &mut String,
    netlist: &PhysicalNetlist,
    _symbol_table: &SymbolTable,
    physical_graph: &PhysicalNetlistGraph,
) -> Result<(), Box<dyn std::error::Error>> {
    if netlist.devices.is_empty() {
        return Ok(());
    }

    netlist_str.push_str("\n* ========================================\n");
    netlist_str.push_str("* EXTRACTED DEVICES\n");
    netlist_str.push_str("* ========================================\n");

    for device in &netlist.devices {
        let dev_type_lower = device.device_type.to_lowercase();
        let resolve_term = |name: &str| -> String {
            let key = (device.name.to_string(), name.to_string());
            if let Some(node) = physical_graph.device_nodes.get(&key) {
                node.clone()
            } else if let Some(net) = device.terminals.get(name) {
                net.to_string()
            } else {
                "0".to_string()
            }
        };

        if dev_type_lower.contains("nmos") || dev_type_lower == "nmos" {
            let d = resolve_term("D");
            let g = resolve_term("G");
            let s = resolve_term("S");
            let b = resolve_term("B");

            let mut params_str = String::new();
            if let Some(w) = device.params.get("W") {
                params_str.push_str(&format!(" W={}", format_measurement_spice(w)));
            }
            if let Some(l) = device.params.get("L") {
                params_str.push_str(&format!(" L={}", format_measurement_spice(l)));
            }

            netlist_str.push_str(&format!(
                "X{} {} {} {} {} sky130_fd_pr__nmos_01v8{}\n",
                device.name, d, g, s, b, params_str
            ));
        } else if dev_type_lower.contains("pmos") || dev_type_lower == "pmos" {
            let d = resolve_term("D");
            let g = resolve_term("G");
            let s = resolve_term("S");
            let b = resolve_term("B");

            let mut params_str = String::new();
            if let Some(w) = device.params.get("W") {
                params_str.push_str(&format!(" W={}", format_measurement_spice(w)));
            }
            if let Some(l) = device.params.get("L") {
                params_str.push_str(&format!(" L={}", format_measurement_spice(l)));
            }

            netlist_str.push_str(&format!(
                "X{} {} {} {} {} sky130_fd_pr__pmos_01v8{}\n",
                device.name, d, g, s, b, params_str
            ));
        } else if dev_type_lower.starts_with('r') || dev_type_lower.contains("res") {
            let term_names: Vec<&CompactString> = device.terminals.keys().collect();
            let n1 = term_names.get(0).map(|t| resolve_term(t.as_str())).unwrap_or_else(|| "0".into());
            let n2 = term_names.get(1).map(|t| resolve_term(t.as_str())).unwrap_or_else(|| "0".into());
            let val = device.params.get("R").or_else(|| device.params.get("value"))
                .map(format_measurement_spice)
                .unwrap_or_else(|| "1000".into());
            netlist_str.push_str(&format!("R{} {} {} {}\n", device.name, n1, n2, val));
        } else if dev_type_lower.starts_with('c') || dev_type_lower.contains("cap") {
            let term_names: Vec<&CompactString> = device.terminals.keys().collect();
            let n1 = term_names.get(0).map(|t| resolve_term(t.as_str())).unwrap_or_else(|| "0".into());
            let n2 = term_names.get(1).map(|t| resolve_term(t.as_str())).unwrap_or_else(|| "0".into());
            let val = device.params.get("C").or_else(|| device.params.get("value"))
                .map(format_measurement_spice)
                .unwrap_or_else(|| "1e-12".into());
            netlist_str.push_str(&format!("C{} {} {} {}\n", device.name, n1, n2, val));
        } else {
            // Generic subcircuit call
            let mut terms = Vec::new();
            for (t_name, _) in &device.terminals {
                terms.push(resolve_term(t_name.as_str()));
            }
            let mut params = Vec::new();
            for (p_name, p_val) in &device.params {
                params.push(format!("{}={}", p_name, format_measurement_spice(p_val)));
            }
            netlist_str.push_str(&format!(
                "X{} {} {} {}\n",
                device.name,
                terms.join(" "),
                device.device_type,
                params.join(" ")
            ));
        }
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
