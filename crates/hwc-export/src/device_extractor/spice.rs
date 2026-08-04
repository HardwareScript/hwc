use compact_str::CompactString;
use hwc_engine::HardwareSpace;

/// Format device instances as SPICE text
///
/// Reads from HardwareSpace.device_instances (populated during compilation)
/// and formats them according to SPICE3/ngspice standards.
///
/// Supports:
/// - Resistors (R): R<name> <nodeA> <nodeB> <value_ohms>
/// - Capacitors (C): C<name> <nodeA> <nodeB> <value_farads>
/// - Inductors (L): L<name> <nodeA> <nodeB> <value_henries>
/// - Diodes (D): D<name> <anode> <cathode> <model>
/// - MOSFETs (M): M<name> <drain> <gate> <source> <bulk> <model> W=<w>u L=<l>u
/// - Subcircuits (X): X<name> <nodes...> <subckt_name>
pub fn format_spice(space: &HardwareSpace) -> CompactString {
    let mut spice = String::new();

    // Header comments
    spice.push_str("* Hardware Script Native Netlist\n");
    spice.push_str("* Format: SPICE3 / ngspice\n\n");

    // Format each device from the registry
    for device in &space.device_instances {
        let device_type = device.device_type.as_str();
        let nc = "nc".into();

        // Match on device category/type
        match device_type {
            // ================================================================
            // RESISTOR CARD: R<name> <nodeA> <nodeB> <value_ohms>
            // ================================================================
            "Resistor" | "PolyResistor" | "R" => {
                let node_a = device.terminal_nets.get("A").unwrap_or(&nc);
                let node_b = device.terminal_nets.get("B").unwrap_or(&nc);
                let r_val = device.parameters.get("R").copied().unwrap_or(400.0);

                spice.push_str(&format!(
                    "R{} {} {} {:.2}\n",
                    device.name, node_a, node_b, r_val
                ));
            }

            // ================================================================
            // CAPACITOR CARD: C<name> <nodeA> <nodeB> <value_farads>
            // ================================================================
            "Capacitor" | "C" => {
                let top = device.terminal_nets.get("top")
                    .or_else(|| device.terminal_nets.get("A"))
                    .unwrap_or(&nc);
                let bottom = device.terminal_nets.get("bottom")
                    .or_else(|| device.terminal_nets.get("B"))
                    .unwrap_or(&nc);
                let c_val = device.parameters.get("C").copied().unwrap_or(1e-12);

                spice.push_str(&format!(
                    "C{} {} {} {:.2e}\n",
                    device.name, top, bottom, c_val
                ));
            }

            // ================================================================
            // INDUCTOR CARD: L<name> <nodeA> <nodeB> <value_henries>
            // ================================================================
            "Inductor" | "L" => {
                let node_a = device.terminal_nets.get("A").unwrap_or(&nc);
                let node_b = device.terminal_nets.get("B").unwrap_or(&nc);
                let l_val = device.parameters.get("L").copied().unwrap_or(1e-9);

                spice.push_str(&format!(
                    "L{} {} {} {:.2e}\n",
                    device.name, node_a, node_b, l_val
                ));
            }

            // ================================================================
            // DIODE CARD: D<name> <anode> <cathode> <model>
            // ================================================================
            "Diode" | "D" => {
                let anode = device.terminal_nets.get("anode")
                    .or_else(|| device.terminal_nets.get("A"))
                    .unwrap_or(&nc);
                let cathode = device.terminal_nets.get("cathode")
                    .or_else(|| device.terminal_nets.get("K"))
                    .unwrap_or(&nc);

                spice.push_str(&format!(
                    "D{} {} {} D1N4148\n",
                    device.name, anode, cathode
                ));
            }

            // ================================================================
            // MOSFET CARD: M<name> <drain> <gate> <source> <bulk> <model> W=<w>u L=<l>u
            // ================================================================
            "NMOS" | "PMOS" | "MOSFET" | "Transistor" => {
                let drain = device.terminal_nets.get("drain").unwrap_or(&nc);
                let gate = device.terminal_nets.get("gate").unwrap_or(&nc);
                let source = device.terminal_nets.get("source").unwrap_or(&nc);
                let bulk = device.terminal_nets.get("bulk").unwrap_or(&nc);

                let w = device.parameters.get("W").copied().unwrap_or(1.0);
                let l = device.parameters.get("L").copied().unwrap_or(0.18);

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

            // ================================================================
            // GENERIC SUBCIRCUIT FALLBACK: X<name> <nodes...> <subckt_name>
            // ================================================================
            _ => {
                spice.push_str(&format!("X{} ", device.name));
                for (_term_name, net_name) in &device.terminal_nets {
                    spice.push_str(&format!("{} ", net_name));
                }
                spice.push_str(&format!("{}\n", device_type));
            }
        }
    }

    spice.push_str("\n* End of Netlist\n");
    spice.into()
}
