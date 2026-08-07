//! Emits SPICE cards for extracted physical devices and integrated trace parasitics.

use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_parser::SpiceParameterStyle;

use super::types::PhysicalNetlistGraph;

/// Emit EXTRACTED DEVICES (Intent-Based Atom Architecture).
///
/// Devices are extracted from explicit device: bindings during alignment validation.
/// This section outputs devices using their SPICE metadata from device definitions.
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
        // Get device type name from the registry
        let device_type_name = netlist
            .device_registry
            .get_name(device.device_type_id)
            .ok_or_else(|| {
                format!(
                    "Device '{}' has invalid device_type_id: {}",
                    device.name, device.device_type_id
                )
            })?;

        // Look up device definition from symbol table to get SPICE metadata
        let device_def = symbol_table.get_device(device_type_name).map_err(|e| {
            format!(
                "Device '{}' of type '{}' not found in symbol table: {}",
                device.name, device_type_name, e
            )
        })?;

        // Get SPICE export info - ERROR if not defined
        let spice_info = device_def.spice_info().ok_or_else(|| {
            format!(
                "Device '{}' of type '{}' has no SPICE export metadata.\n\
                 \n\
                 Add a 'spice:' block to the device definition:\n\
                 \n\
                 device {}:\n\
                     terminals: [...]\n\
                     materials: ...\n\
                     spice:\n\
                         prefix: <char>  # R, C, L, M, D, etc.\n\
                         terminal_order: [terminal1, terminal2, ...]\n\
                         parameters: [param1, param2, ...]  # optional\n\
                         model: ModelName  # optional",
                device.name, device_type_name, device_type_name
            )
        })?;

        // Build SPICE card using metadata from device definition
        // If subcircuit is defined, use X prefix and subcircuit call
        // Otherwise use the device's prefix directly
        if let Some(ref _subcircuit_name) = spice_info.subcircuit {
            // PDK subcircuit mode: XR1 n1 n2 nGND sky130_fd_pr__res_high_po W=1.0u L=4.0u
            netlist_str.push('X');
            netlist_str.push_str(&device.name);
        } else {
            // Direct device mode: R1 n1 n2 1000
            netlist_str.push(spice_info.prefix);
            netlist_str.push_str(&device.name);
        }

        // Add terminals using physical nodes from the graph
        for terminal_name in &spice_info.terminal_order {
            // Zero Compiler Magic: FAIL LOUDLY if terminal is unbound
            // Every terminal in terminal_order MUST have an explicit binding
            let key = (device.name.to_string(), terminal_name.to_string());
            let physical_node = if let Some(node) = physical_graph.device_nodes.get(&key) {
                node.clone()
            } else {
                // Fallback to logical net name if no physical node
                device.terminals.get(terminal_name.as_str()).ok_or_else(|| {
                    format!(
                        "\n❌ UNBOUND DEVICE TERMINAL\n\
                         \nDevice '{}' (type: {}) requires terminal '{}' in its SPICE terminal_order.\n\
                         \nRequired terminals: {:?}\n\
                         Available bindings: {:?}\n\
                         \n💡 FIX: Add an explicit binding in your layout:\n\
                         \n   add pour(...) named {}_{} on layer: ...:\n\
                                device: {}.{}\n\
                                net: <your_net_name>  # e.g., GND for BULK terminals\n\
                         \n📖 Zero Compiler Magic: HardwareScript never guesses terminal connections.\n\
                            Every terminal must be explicitly declared by the user.\n",
                        device.name,
                        device_type_name,
                        terminal_name,
                        spice_info.terminal_order,
                        device.terminals.keys().collect::<Vec<_>>(),
                        device.name,
                        terminal_name,
                        device.name,
                        terminal_name
                    )
                })?
                .to_string()
            };

            netlist_str.push(' ');
            netlist_str.push_str(&physical_node);
        }

        // Add subcircuit name and parameters if in subcircuit mode
        if let Some(ref subcircuit_name) = spice_info.subcircuit {
            // PDK Subcircuit Mode: XR1 n1 n2 GND sky130_fd_pr__res_high_po W=1.0u L=4.0u
            // Get the subcircuit definition to know all terminals
            let subcircuit_def = symbol_table.get_subcircuit(subcircuit_name).map_err(|e| {
                format!(
                    "Device '{}' references subcircuit '{}' which is not defined: {}",
                    device.name, subcircuit_name, e
                )
            })?;

            netlist_str.push(' ');
            netlist_str.push_str(subcircuit_name);

            // Add subcircuit parameters using named style (W=1.0u L=4.0u)
            // These come from the device's calculated geometry
            for param in &subcircuit_def.parameters {
                let param_value = device.parameters.get(param.name.as_str()).ok_or_else(|| {
                    format!(
                        "Device '{}' missing parameter '{}' required by subcircuit '{}'",
                        device.name, param.name, subcircuit_name
                    )
                })?;

                // Format with SI prefix (u for micro, m for milli, etc.)
                netlist_str.push_str(&format!(" {}={:.2}u", param.name, param_value));
            }
        } else if let Some(ref model) = spice_info.model_name {
            // Add model name if specified (and not in subcircuit mode)
            netlist_str.push(' ');
            netlist_str.push_str(model);

            // Add parameters for non-subcircuit devices
            for param_name in &spice_info.parameters {
                let param_value = device.parameters.get(param_name.as_str()).ok_or_else(|| {
                    format!(
                        "Device '{}' missing required parameter '{}' (device type: {})",
                        device.name, param_name, device_type_name
                    )
                })?;

                match spice_info.parameter_style {
                    SpiceParameterStyle::Positional => {
                        // Positional values: R1 n1 n2 1000
                        if param_value.abs() < 1e-3 || param_value.abs() > 1e6 {
                            netlist_str.push_str(&format!(" {:.2e}", param_value));
                        } else {
                            netlist_str.push_str(&format!(" {:.2}", param_value));
                        }
                    }
                    SpiceParameterStyle::Named => {
                        // Named parameters: M1 d g s b NMOS W=1u L=0.18u
                        netlist_str.push_str(&format!(" {}={:.2}u", param_name, param_value));
                    }
                }
            }
        } else {
            // Flat device mode (no model, no subcircuit) - just parameters
            for param_name in &spice_info.parameters {
                let param_value = device.parameters.get(param_name.as_str()).ok_or_else(|| {
                    format!(
                        "Device '{}' missing required parameter '{}' (device type: {})",
                        device.name, param_name, device_type_name
                    )
                })?;

                match spice_info.parameter_style {
                    SpiceParameterStyle::Positional => {
                        // Positional values: R1 n1 n2 1000
                        if param_value.abs() < 1e-3 || param_value.abs() > 1e6 {
                            netlist_str.push_str(&format!(" {:.2e}", param_value));
                        } else {
                            netlist_str.push_str(&format!(" {:.2}", param_value));
                        }
                    }
                    SpiceParameterStyle::Named => {
                        // Named parameters: M1 d g s b NMOS W=1u L=0.18u
                        netlist_str.push_str(&format!(" {}={:.2}u", param_name, param_value));
                    }
                }
            }
        }

        netlist_str.push('\n');
    }
    netlist_str.push('\n');

    Ok(())
}

/// Emit integrated trace parasitics (R, C) into the netlist body.
pub fn emit_parasitics(netlist_str: &mut String, physical_graph: &PhysicalNetlistGraph) {
    eprintln!(
        "[NETLIST PARASITIC DEBUG] About to check if parasitics should be written: {} parasitics",
        physical_graph.parasitics.len()
    );
    if physical_graph.parasitics.is_empty() {
        return;
    }

    eprintln!(
        "[NETLIST PARASITIC DEBUG] Writing {} parasitics to SPICE netlist",
        physical_graph.parasitics.len()
    );
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
                netlist_str.push_str("* Trace resistance\n");
                netlist_str.push_str(&format!(
                    "R{} {} {} {:.6e}\n",
                    name, node_a, node_b, value_ohms
                ));
            }
            super::types::ParasiticElement::GroundCapacitance {
                name,
                node,
                ref_node,
                value_farads,
            } => {
                netlist_str.push_str("* Ground capacitance\n");
                netlist_str.push_str(&format!(
                    "C{} {} {} {:.6e}\n",
                    name, node, ref_node, value_farads
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
