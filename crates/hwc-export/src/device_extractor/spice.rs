use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Format device instances as SPICE text
pub fn format_spice(
    space: &HardwareSpace,
    _symbol_table: &SymbolTable,
) -> Result<CompactString, String> {
    let mut spice = String::new();

    spice.push_str("* Hardware Script Native Netlist\n");
    spice.push_str("* Format: SPICE3 / ngspice\n\n");

    for device in &space.device_instances {
        let name = &device.name;
        let dev_type = device.device_type.to_lowercase();
        let terms: Vec<String> = device.terminal_nets.values().map(|v| v.to_string()).collect();
        let params: Vec<String> = device.parameters.iter().map(|(k, v)| format!("{}={:.4e}", k, v)).collect();

        if dev_type.contains("nmos") || dev_type.contains("pmos") {
            spice.push_str(&format!("X{} {} {} {}\n", name, terms.join(" "), device.device_type, params.join(" ")));
        } else if dev_type.starts_with('r') {
            spice.push_str(&format!("R{} {}\n", name, terms.join(" ")));
        } else if dev_type.starts_with('c') {
            spice.push_str(&format!("C{} {}\n", name, terms.join(" ")));
        } else {
            spice.push_str(&format!("X{} {} {} {}\n", name, terms.join(" "), device.device_type, params.join(" ")));
        }
    }

    spice.push_str("\n* End of Netlist\n");
    Ok(spice.into())
}
