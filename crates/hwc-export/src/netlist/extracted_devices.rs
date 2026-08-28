//! Emits SPICE cards for extracted physical devices and integrated trace parasitics.

use compact_str::CompactString;
use hwc_compiler::eval::{MeasurementValue, UnitDimension};
use hwc_compiler::SymbolTable;

use super::types::{PhysicalNetlist, PhysicalNetlistGraph};

fn format_measurement_spice_unit(m: &MeasurementValue) -> String {
    match m.dimension {
        UnitDimension::Length => {
            let um = (m.raw as f64) * 1e-6;
            format!("{:.2}u", um)
        }
        UnitDimension::Resistance => {
            let ohms = (m.raw as f64) * 1e-6;
            format!("{:.2}", ohms)
        }
        UnitDimension::Capacitance => {
            let pf = (m.raw as f64) * 1e-6;
            format!("{:.2}p", pf)
        }
        UnitDimension::Inductance => {
            let nh = (m.raw as f64) * 1e-3;
            format!("{:.2}n", nh)
        }
        UnitDimension::Voltage => {
            let v = (m.raw as f64) * 1e-9;
            format!("{:.2}V", v)
        }
        UnitDimension::Current => {
            let ua = (m.raw as f64) * 1e-6;
            format!("{:.2}uA", ua)
        }
        _ => format!("{}", m.raw),
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

        // 1. Look up device definition from symbol table
        let device_decl = symbol_table.get_device(&device.device_type).ok();

        let mut prefix = "X".to_string();
        let mut subcircuit_name = None;
        let mut terminal_order: Vec<CompactString> = Vec::new();
        let mut param_names: Vec<CompactString> = Vec::new();
        let mut param_style = "named".to_string();

        if let Some(decl) = device_decl {
            for sec in &decl.sections {
                if sec.name == "terminals" {
                    for (_, expr) in &sec.fields {
                        if let hwc_parser::ast::Expression::ArrayLiteral { elements, .. } = expr {
                            for elem in elements {
                                if let hwc_parser::ast::Expression::Variable { name, .. } = elem {
                                    terminal_order.push(name.clone());
                                }
                            }
                        }
                    }
                } else if sec.name == "spice" {
                    for (fname, fexpr) in &sec.fields {
                        match fname.as_str() {
                            "prefix" => {
                                if let hwc_parser::ast::Expression::StringLiteral { value, .. } = fexpr {
                                    prefix = value.to_string();
                                }
                            }
                            "subcircuit" => {
                                if let hwc_parser::ast::Expression::StringLiteral { value, .. } = fexpr {
                                    subcircuit_name = Some(value.to_string());
                                }
                            }
                            "terminal_order" => {
                                if let hwc_parser::ast::Expression::ArrayLiteral { elements, .. } = fexpr {
                                    terminal_order.clear();
                                    for elem in elements {
                                        if let hwc_parser::ast::Expression::Variable { name, .. } = elem {
                                            terminal_order.push(name.clone());
                                        }
                                    }
                                }
                            }
                            "parameters" => {
                                if let hwc_parser::ast::Expression::ArrayLiteral { elements, .. } = fexpr {
                                    for elem in elements {
                                        if let hwc_parser::ast::Expression::Variable { name, .. } = elem {
                                            param_names.push(name.clone());
                                        }
                                    }
                                }
                            }
                            "parameter_style" => {
                                if let hwc_parser::ast::Expression::StringLiteral { value, .. } = fexpr {
                                    param_style = value.to_string();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let dev_type_lower = device.device_type.to_lowercase();

        if subcircuit_name.is_none() {
            if dev_type_lower == "resistor" || dev_type_lower == "sky130_fd_pr__res_high_po" {
                subcircuit_name = Some("sky130_fd_pr__res_high_po".to_string());
                terminal_order = vec!["A".into(), "B".into(), "BULK".into()];
                param_names = vec!["W".into(), "L".into()];
                param_style = "named".to_string();
            }
        }

        if terminal_order.is_empty() {
            terminal_order = device.terminals.keys().cloned().collect();
        }

        // 2. If subcircuit is specified, emit subcircuit call (X-prefix)
        if let Some(subckt) = subcircuit_name {
            let terms_str = terminal_order
                .iter()
                .map(|t| resolve_term(t.as_str()))
                .collect::<Vec<_>>()
                .join(" ");

            let mut params_str = String::new();
            if !param_names.is_empty() {
                for p in &param_names {
                    if let Some(val) = device.params.get(p) {
                        if param_style == "named" {
                            params_str.push_str(&format!(" {}={}", p, format_measurement_spice_unit(val)));
                        } else {
                            params_str.push_str(&format!(" {}", format_measurement_spice_unit(val)));
                        }
                    }
                }
            } else {
                for (p, val) in &device.params {
                    params_str.push_str(&format!(" {}={}", p, format_measurement_spice_unit(val)));
                }
            }

            netlist_str.push_str(&format!(
                "{}{} {} {}{}\n",
                prefix, device.name, terms_str, subckt, params_str
            ));
        } else if dev_type_lower.contains("nmos") || dev_type_lower == "nmos" {
            let d = resolve_term("D");
            let g = resolve_term("G");
            let s = resolve_term("S");
            let b = resolve_term("B");

            let mut params_str = String::new();
            if let Some(w) = device.params.get("W") {
                params_str.push_str(&format!(" W={}", format_measurement_spice_unit(w)));
            }
            if let Some(l) = device.params.get("L") {
                params_str.push_str(&format!(" L={}", format_measurement_spice_unit(l)));
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
                params_str.push_str(&format!(" W={}", format_measurement_spice_unit(w)));
            }
            if let Some(l) = device.params.get("L") {
                params_str.push_str(&format!(" L={}", format_measurement_spice_unit(l)));
            }

            netlist_str.push_str(&format!(
                "X{} {} {} {} {} sky130_fd_pr__pmos_01v8{}\n",
                device.name, d, g, s, b, params_str
            ));
        } else {
            // Generic subcircuit call
            let terms_str = terminal_order
                .iter()
                .map(|t| resolve_term(t.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            let mut params = Vec::new();
            for (p_name, p_val) in &device.params {
                params.push(format!("{}={}", p_name, format_measurement_spice_unit(p_val)));
            }
            netlist_str.push_str(&format!(
                "X{} {} {} {}\n",
                device.name,
                terms_str,
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
