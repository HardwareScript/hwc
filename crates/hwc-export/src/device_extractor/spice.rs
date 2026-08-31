use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Format device instances as SPICE text
pub fn format_spice(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
) -> Result<CompactString, String> {
    let mut spice = String::new();

    spice.push_str("* Hardware Script Native Netlist\n");
    spice.push_str("* Format: SPICE3 / ngspice\n\n");

    for device in &space.device_instances {
        let name = &device.name;

        let device_decl = symbol_table
            .get_device(&device.device_type)
            .map_err(|_| format!("FATAL: Device definition '{}' not found in SymbolTable", device.device_type))?;

        let spice_decl = device_decl.spice();
        let prefix = spice_decl.prefix.as_deref().unwrap_or("X");
        let subcircuit = spice_decl.subcircuit.as_deref().unwrap_or(&device.device_type);
        let mut terminal_order = spice_decl.terminal_order;
        if terminal_order.is_empty() {
            terminal_order = device.terminals.clone();
        }

        let mut terms = Vec::with_capacity(terminal_order.len());
        for term_name in &terminal_order {
            let net = device.terminal_nets.get(term_name)
                .ok_or_else(|| format!("FATAL: Device '{}' missing connection for terminal '{}'", name, term_name))?;
            terms.push(net.as_str());
        }

        let params: Vec<String> = device.parameters.iter().map(|(k, v)| format!("{}={:.4e}", k.to_lowercase(), v)).collect();

        if prefix == "X" {
            spice.push_str(&format!("X{} {} {} {}\n", name, terms.join(" "), subcircuit, params.join(" ")));
        } else {
            spice.push_str(&format!("{}{} {} {}\n", prefix, name, terms.join(" "), params.join(" ")));
        }
    }

    spice.push_str("\n* End of Netlist\n");
    Ok(spice.into())
}
