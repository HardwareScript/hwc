//! Generates the reusable SPICE circuit body (DUT) shared by all variants.
//!
//! Emits PDK subcircuit definitions, net comments, schematic-level components,
//! and delegates extracted-device and parasitic emission to sibling modules.

use compact_str::CompactString;
use super::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashSet;

use super::extracted_devices::{emit_extracted_devices, emit_parasitics};

/// Format a Z coordinate with appropriate unit selection for maximum precision.
///
/// **Unit Selection Rules (v0.2.2 - External Audit Precision Fix):**
/// - Values >= 1000nm: Format as micrometers (µm) with up to 6 decimal places
/// - Values < 1000nm: Format as nanometers (nm) as integer
///
/// This avoids precision loss from rounding (e.g., 380nm → 0.38µm exact, not 0.0004mm).
fn format_z_coordinate(z_nm: i64) -> String {
    if z_nm.abs() >= 1000 {
        // Format as micrometers for readability when >= 1µm
        let um = z_nm as f64 / 1000.0;
        let formatted = format!("{:.6}", um);
        // Trim trailing zeros and decimal point if no fractional part
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{}µm", trimmed)
    } else {
        // Keep as nanometers for sub-micron precision
        format!("{}nm", z_nm)
    }
}

/// Generate the circuit body (devices and nets) - reused by all SPICE variants
pub fn generate_circuit_body(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    physical_graph: &PhysicalNetlistGraph,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut netlist_str = String::new();

    eprintln!(
        "[NETLIST DEBUG] physical_netlist is_some: {}",
        physical_netlist.is_some()
    );
    if let Some(netlist) = physical_netlist {
        eprintln!(
            "[NETLIST DEBUG] physical_netlist.devices.len(): {}",
            netlist.devices.len()
        );
    }

    // **Stage 1 PDK Subcircuit Cards**
    // Emit .subckt definitions for PDK models used by devices
    // This section comes first so subcircuits are defined before they're instantiated
    if let Some(netlist) = physical_netlist {
        emit_pdk_subcircuits(&mut netlist_str, netlist, symbol_table, unit_registry)?;
    }

    // Emit nets as comments for reference
    emit_net_comments(&mut netlist_str, space);

    // Emit components as SPICE devices
    // If a physical netlist is present, skip schematic-level components to avoid
    // conflicting with extracted M devices (prevents LTspice subcircuit conflicts)
    let is_physical_mode = physical_netlist.is_some();
    emit_components(&mut netlist_str, space, symbol_table, is_physical_mode)?;

    // GAP 7 Phase 4: EXTRACTED DEVICES (Intent-Based Atom Architecture)
    // Devices are extracted from explicit device: bindings during alignment validation
    // This section outputs devices using their SPICE metadata from device definitions
    if let Some(netlist) = physical_netlist {
        emit_extracted_devices(&mut netlist_str, netlist, symbol_table, physical_graph)?;

        // Emit parasitics integrated into the netlist
        emit_parasitics(&mut netlist_str, physical_graph);
    } else {
        // No physical netlist available (Artist Mode or no device bindings)
        println!("   ├─ Device extraction skipped (requires module with explicit bindings)");
        println!("   ├─ Use 'device: DeviceName.terminal' to bind pours to devices");
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* DEVICE EXTRACTION REQUIRES EXPLICIT BINDINGS\n");
        netlist_str.push_str("* Use 'device: DeviceName.terminal' property\n");
        netlist_str.push_str("* ========================================\n\n");
    }

    Ok(netlist_str)
}

/// Emit `.subckt` definitions for every PDK subcircuit referenced by a device.
fn emit_pdk_subcircuits(
    netlist_str: &mut String,
    netlist: &PhysicalNetlist,
    symbol_table: &SymbolTable,
    _unit_registry: &hwc_types::UnitRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut emitted_subckts = rustc_hash::FxHashSet::default();

    for dev in &netlist.devices {
        let device_decl = symbol_table.get_device(&dev.device_type).ok();
        let mut subcircuit_name = None;
        if let Some(decl) = device_decl {
            for sec in &decl.sections {
                if sec.name == "spice" {
                    for (fname, fexpr) in &sec.fields {
                        if fname.as_str() == "subcircuit" {
                            if let hwc_parser::ast::Expression::StringLiteral { value, .. } = fexpr {
                                subcircuit_name = Some(value.to_string());
                            }
                        }
                    }
                }
            }
        }

        if let Some(subckt) = subcircuit_name {
            if emitted_subckts.insert(subckt.clone()) {
                netlist_str.push_str("* ========================================\n");
                netlist_str.push_str(&format!("* PDK SUBCIRCUIT: {}\n", subckt));
                netlist_str.push_str("* ========================================\n");

                if subckt == "sky130_fd_pr__res_high_po" {
                    netlist_str.push_str(".subckt sky130_fd_pr__res_high_po A B BULK W=1u L=1u\n");
                    netlist_str.push_str("RR_head A node_1 362\n");
                    netlist_str.push_str("RR_tail node_2 B 362\n");
                    netlist_str.push_str("RR_body node_1 node_2 {3.500000e2 * (L / W)} tc1=-0.00147 tc2=0.0000027 vc1=-0.00032 vc2=0.000018\n");
                    netlist_str.push_str("CC_sub1 A BULK {2.000000e-15 * W * L}\n");
                    netlist_str.push_str("CC_sub2 B BULK {2.000000e-15 * W * L}\n");
                    netlist_str.push_str(".ends sky130_fd_pr__res_high_po\n\n");
                } else if subckt.contains("nfet") || subckt.contains("nmos") {
                    netlist_str.push_str(&format!("* Subcircuit '{}' uses foundry model\n", subckt));
                    netlist_str.push_str(&format!(".include \"sky130_fd_pr/models/{}.model.spice\"\n\n", subckt));
                } else if subckt.contains("pfet") || subckt.contains("pmos") {
                    netlist_str.push_str(&format!("* Subcircuit '{}' uses foundry model\n", subckt));
                    netlist_str.push_str(&format!(".include \"sky130_fd_pr/models/{}.model.spice\"\n\n", subckt));
                }
            }
        }
    }

    Ok(())
}

/// Emit nets as reference comments (pours + netlist arena connections).
fn emit_net_comments(netlist_str: &mut String, space: &HardwareSpace) {
    // Collect nets from pours
    let mut pour_nets: FxHashSet<CompactString> = FxHashSet::default();
    for pour in &space.pours {
        if let Some(ref net_name) = pour.net {
            pour_nets.insert(net_name.clone());
        }
    }

    let net_count = space.netlist.num_nets();
    let total_nets = net_count + pour_nets.len();

    if total_nets > 0 {
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* NETS\n");
        netlist_str.push_str("* ========================================\n");

        // Emit nets from pours (silicon regions, metal layers)
        // Group merged regions together for parasitic extraction
        let mut merged_regions: rustc_hash::FxHashMap<
            CompactString,
            Vec<&hwc_engine::space::PourMetadata>,
        > = rustc_hash::FxHashMap::default();
        let mut standalone_pours = Vec::new();

        for pour in &space.pours {
            if let Some(ref merged_id) = pour.merged_region_id {
                merged_regions
                    .entry(merged_id.clone())
                    .or_default()
                    .push(pour);
            } else {
                standalone_pours.push(pour);
            }
        }

        // Emit merged regions (treat as single electrical node)
        for (merged_id, pours) in &merged_regions {
            if let Some(first_pour) = pours.first() {
                if let Some(ref net_name) = first_pour.net {
                    // Calculate total area for merged region
                    let total_area: i64 = pours.iter().map(|p| p.area_nm2).sum();

                    netlist_str.push_str(&format!(
                        "* Net: {} (merged region: {}, {} instances, total area: {} nm², material: {}, z: {})\n",
                        net_name,
                        merged_id,
                        pours.len(),
                        total_area,
                        first_pour.material_name,
                        format_z_coordinate(first_pour.z_bottom_nm)
                    ));
                    netlist_str
                        .push_str("*   Parasitic extraction: Treat as single electrical node\n");
                }
            }
        }

        // Emit standalone pours
        for pour in standalone_pours {
            if let Some(ref net_name) = pour.net {
                netlist_str.push_str(&format!(
                    "* Net: {} (pour: {}, material: {}, z: {})\n",
                    net_name,
                    pour.name,
                    pour.material_name,
                    format_z_coordinate(pour.z_bottom_nm)
                ));
            }
        }

        // Emit nets from netlist arena (discrete component connections)
        for net_id in space.netlist.all_net_ids() {
            if let Some(net) = space.netlist.get_net(net_id) {
                let material_name = space
                    .material_registry
                    .get_name(net.material)
                    .unwrap_or("Unknown");
                netlist_str.push_str(&format!(
                    "* Net: {} (width={}nm, material={})\n",
                    net.name, net.width_nm, material_name
                ));

                // List connected pins
                if !net.pins.is_empty() {
                    netlist_str.push_str("*   Connected pins:\n");
                    for pin_id in &net.pins {
                        if let Some(pin) = space.netlist.get_pin(*pin_id) {
                            if let Some(comp) = space.netlist.get_component(pin.parent_component) {
                                netlist_str
                                    .push_str(&format!("*     - {}.{}\n", comp.name, pin.name));
                            }
                        }
                    }
                }
            }
        }
        netlist_str.push('\n');
    }
}

/// Emit schematic-level components from space.netlist as SPICE subcircuits
fn emit_components(
    netlist_str: &mut String,
    space: &HardwareSpace,
    _symbol_table: &SymbolTable,
    is_physical_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_physical_mode {
        return Ok(());
    }

    if !space.device_instances.is_empty() {
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* DEVICE INSTANCES\n");
        netlist_str.push_str("* ========================================\n");
        for dev in &space.device_instances {
            netlist_str.push('X');
            netlist_str.push_str(&dev.name);
            for term in &dev.terminals {
                let net = dev.terminal_nets.get(term).map(|s| s.as_str()).unwrap_or("0");
                netlist_str.push(' ');
                netlist_str.push_str(net);
            }
            netlist_str.push(' ');
            netlist_str.push_str(&dev.device_type);
            for (k, v) in &dev.parameters {
                netlist_str.push_str(&format!(" {}={}", k, v));
            }
            netlist_str.push('\n');
        }
    }

    let component_count = space.netlist.component_count();
    if component_count == 0 {
        return Ok(());
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* COMPONENTS (Schematic-Level Subcircuits)\n");
    netlist_str.push_str("* ========================================\n");

    for i in 0..component_count {
        let comp_id = hwc_engine::netlist::ComponentId::new(i as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
            let pins = space.netlist.get_component_pins(comp_id);

            let mut pin_net_map = rustc_hash::FxHashMap::default();
            for pin_id in &pins {
                if let Some(pin) = space.netlist.get_pin(*pin_id) {
                    let entity_graph_net = space
                        .entity_graph
                        .get_component_pins()
                        .iter()
                        .find(|vp| vp.component_name == component.name && vp.pin_name == pin.name)
                        .and_then(|vp| vp.net.clone());

                    if let Some(net_name) = entity_graph_net {
                        pin_net_map.insert(pin.name.to_string(), net_name.to_string());
                    } else if let Some(net_id) = pin.connected_net {
                        if let Some(net) = space.netlist.get_net(net_id) {
                            pin_net_map.insert(pin.name.to_string(), net.name.to_string());
                        }
                    }
                }
            }

            netlist_str.push('X');
            netlist_str.push_str(&component.name);
            for pin_id in &pins {
                if let Some(pin) = space.netlist.get_pin(*pin_id) {
                    let net = pin_net_map.get(pin.name.as_str()).map(|s| s.as_str()).unwrap_or("0");
                    netlist_str.push(' ');
                    netlist_str.push_str(net);
                }
            }
            netlist_str.push(' ');
            netlist_str.push_str(&component.component_type);
            netlist_str.push('\n');
        }
    }
    netlist_str.push('\n');
    Ok(())
}
