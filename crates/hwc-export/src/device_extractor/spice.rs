use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_parser::ast::device::SpiceParameterStyle;

/// Format device instances as SPICE text using device contracts
///
/// **NO HARDCODING** - All device information comes from the device contract's SpiceExportInfo.
/// This ensures the compiler respects what the .hw files declare, not what we guess.
///
/// If a device is missing SpiceExportInfo, we ERROR LOUDLY rather than guessing.
pub fn format_spice(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
) -> Result<CompactString, String> {
    let mut spice = String::new();
    let mut errors = Vec::new();

    // Header comments
    spice.push_str("* Hardware Script Native Netlist\n");
    spice.push_str("* Format: SPICE3 / ngspice\n\n");

    // Format each device from the registry using its contract
    for device in &space.device_instances {
        // Look up the device contract in the symbol table using get_device()
        let device_def = match symbol_table.get_device(device.device_type.as_str()) {
            Ok(def) => def,
            Err(e) => {
                errors.push(format!(
                    "Device '{}' type '{}' not found in symbol table: {}",
                    device.name, device.device_type, e
                ));
                continue;
            }
        };

        // Get SPICE export info from the device contract
        let spice_info = match device_def.spice_info() {
            Some(info) => info,
            None => {
                errors.push(format!(
                    "Device '{}' (type: {}) missing SPICE export metadata in device contract",
                    device.name, device.device_type
                ));
                continue;
            }
        };

        // Build the SPICE card using contract metadata
        let nc: CompactString = "nc".into();

        // Start with prefix and name: R1, D1, M1, etc.
        spice.push_str(&format!("{}{} ", spice_info.prefix, device.name));

        // Add terminals in the order specified by the contract
        for terminal_name in &spice_info.terminal_order {
            let net = device
                .terminal_nets
                .get(terminal_name.as_str())
                .unwrap_or(&nc);
            spice.push_str(&format!("{} ", net));
        }

        // Add model name if specified
        if let Some(ref model_name) = spice_info.model_name {
            spice.push_str(&format!("{} ", model_name));
        }

        // Add parameters based on style
        match spice_info.parameter_style {
            SpiceParameterStyle::Positional => {
                // Positional: R1 n1 n2 1000
                // Parameters appear as bare values in order
                for param_name in &spice_info.parameters {
                    if let Some(&value) = device.parameters.get(param_name.as_str()) {
                        spice.push_str(&format!("{:.2e} ", value));
                    } else {
                        errors.push(format!(
                            "Device '{}' missing required parameter '{}' (device type: {})",
                            device.name, param_name, device.device_type
                        ));
                    }
                }
            }
            SpiceParameterStyle::Named => {
                // Named: M1 d g s b NMOS W=1u L=0.18u
                // Parameters appear as name=value pairs
                for param_name in &spice_info.parameters {
                    if let Some(&value) = device.parameters.get(param_name.as_str()) {
                        spice.push_str(&format!("{}={:.2e} ", param_name, value));
                    } else {
                        errors.push(format!(
                            "Device '{}' missing required parameter '{}' (device type: {})",
                            device.name, param_name, device.device_type
                        ));
                    }
                }
            }
        }

        spice.push('\n');
    }

    spice.push_str("\n* End of Netlist\n");

    // If any errors occurred, return them as a combined error message
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    Ok(spice.into())
}
