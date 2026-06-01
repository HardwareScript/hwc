use compact_str::CompactString;
use hwc_compiler::alignment::netlist::{DeviceTypeRegistry, PhysicalNetlist};

/// Format physical netlist as SPICE text
///
/// Converts the structured PhysicalNetlist into SPICE netlist format
pub fn format_spice(
    netlist: &PhysicalNetlist,
    device_registry: &DeviceTypeRegistry,
) -> CompactString {
    let mut spice = String::new();

    for device in &netlist.devices {
        let device_type = device_registry
            .get_name(device.device_type_id)
            .unwrap_or("UNKNOWN");

        // ✅ NATIVE v0.1.7 FIX: Only extract active transistors.
        // Do not attempt to extract MOSFET models for raw IC packages or generic components.
        if device_type == "IC_Package" || device_type == "Component" {
            println!("      ⚠️  Skipping SPICE formatting for {}: Not a transistor", device.name);
            continue;
        }

        // Get terminal connections with a default value that lives long enough
        let nc = "nc".into();
        let drain = device.terminals.get("drain").unwrap_or(&nc);
        let gate = device.terminals.get("gate").unwrap_or(&nc);
        let source = device.terminals.get("source").unwrap_or(&nc);
        let bulk = device.terminals.get("bulk").unwrap_or(&nc);

        // Get parameters
        let w = device.parameters.get("W").copied().unwrap_or(0.0);
        let l = device.parameters.get("L").copied().unwrap_or(0.0);

        // Format: M<name> <drain> <gate> <source> <bulk> <type> W=<w>u L=<l>u
        spice.push_str(&format!(
            "M{} {} {} {} {} {} W={}u L={}u",
            device.name, drain, gate, source, bulk, device_type, w, l
        ));

        // Add parasitic parameters if available
        if let Some(&as_val) = device.parameters.get("AS") {
            spice.push_str(&format!(" AS={:.2e}", as_val));
        }
        if let Some(&ad_val) = device.parameters.get("AD") {
            spice.push_str(&format!(" AD={:.2e}", ad_val));
        }
        if let Some(&ps_val) = device.parameters.get("PS") {
            spice.push_str(&format!(" PS={:.2e}", ps_val));
        }
        if let Some(&pd_val) = device.parameters.get("PD") {
            spice.push_str(&format!(" PD={:.2e}", pd_val));
        }

        spice.push('\n');
    }

    spice.into()
}
