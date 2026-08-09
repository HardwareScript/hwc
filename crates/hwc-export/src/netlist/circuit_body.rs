//! Generates the reusable SPICE circuit body (DUT) shared by all variants.
//!
//! Emits PDK subcircuit definitions, net comments, schematic-level components,
//! and delegates extracted-device and parasitic emission to sibling modules.

use compact_str::CompactString;
use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashSet;

use super::extracted_devices::{emit_extracted_devices, emit_parasitics};
use super::subcircuit::generate_spice_subcircuit;
use super::types::PhysicalNetlistGraph;

/// Generate the circuit body (devices and nets) - reused by all SPICE variants
pub fn generate_circuit_body(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    physical_graph: &PhysicalNetlistGraph,
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
        emit_pdk_subcircuits(&mut netlist_str, netlist, symbol_table)?;
    }

    // Emit nets as comments for reference
    emit_net_comments(&mut netlist_str, space);

    // Emit components as SPICE devices
    // If a physical netlist is present, skip schematic-level components to avoid
    // conflicting with extracted M devices (prevents LTspice subcircuit conflicts)
    let is_physical_mode = physical_netlist.is_some();
    emit_components(&mut netlist_str, space, is_physical_mode);

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
) -> Result<(), Box<dyn std::error::Error>> {
    let mut emitted_subcircuits = FxHashSet::default();

    for device in &netlist.devices {
        let device_type_name = netlist
            .device_registry
            .get_name(device.device_type_id)
            .ok_or_else(|| format!("Device '{}' has invalid device_type_id", device.name))?;

        if let Ok(device_def) = symbol_table.get_device(device_type_name) {
            if let Some(spice_info) = device_def.spice_info() {
                if let Some(ref subcircuit_name) = spice_info.subcircuit {
                    // Only emit each subcircuit definition once
                    if emitted_subcircuits.insert(subcircuit_name.clone()) {
                        netlist_str.push_str("* ========================================\n");
                        netlist_str.push_str(&format!("* PDK SUBCIRCUIT: {}\n", subcircuit_name));
                        netlist_str.push_str("* ========================================\n");

                        // Look up the subcircuit definition in the symbol table
                        if let Ok(subckt_def) =
                            symbol_table.get_subcircuit(subcircuit_name.as_str())
                        {
                            // Generate SPICE from typed AST
                            generate_spice_subcircuit(netlist_str, subckt_def)?;
                            netlist_str.push('\n');
                        } else {
                            // Subcircuit referenced but not defined - this is an error
                            return Err(format!(
                                "Device '{}' references subcircuit '{}' which is not defined.\n\
                                 \n\
                                 Add to your PDK file:\n\
                                 \n\
                                 subcircuit {}:\n\
                                     terminals: [{}]\n\
                                     parameters: [W = 1.0um, L = 1.0um]\n\
                                     elements:\n\
                                         R1: Resistor(nodes: [PLUS, MINUS], value: ...)\n\
                                         ...",
                                device.name,
                                subcircuit_name,
                                subcircuit_name,
                                spice_info.terminal_order.join(", ")
                            )
                            .into());
                        }
                    }
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
                        "* Net: {} (merged region: {}, {} instances, total area: {} nm², material: {}, z: {:.4}mm)\n",
                        net_name,
                        merged_id,
                        pours.len(),
                        total_area,
                        first_pour.material_name,
                        first_pour.z_bottom_nm as f64 / 1_000_000.0
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
                    "* Net: {} (pour: {}, material: {}, z: {:.4}mm)\n",
                    net_name,
                    pour.name,
                    pour.material_name,
                    pour.z_bottom_nm as f64 / 1_000_000.0
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

/// Emit schematic-level components as SPICE subcircuit/MOSFET cards.
fn emit_components(netlist_str: &mut String, space: &HardwareSpace, is_physical_mode: bool) {
    let component_count = space.netlist.component_count();
    if component_count == 0 || is_physical_mode {
        return;
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* COMPONENTS (Schematic-Level Subcircuits)\n");
    netlist_str.push_str("* ========================================\n");

    for i in 0..component_count {
        let comp_id = hwc_engine::netlist::ComponentId::new(i as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
            // Get component pins
            let pins = space.netlist.get_component_pins(comp_id);

            // Build net list for this component
            // v0.1.6 Item #13: Use pin net assignments from entity graph (from net: block)
            let mut net_names = Vec::new();
            for pin_id in &pins {
                if let Some(pin) = space.netlist.get_pin(*pin_id) {
                    // First, try to get net assignment from entity graph component pins
                    // (these come from the net: block in component placement)
                    let entity_graph_net = space
                        .entity_graph
                        .get_component_pins()
                        .iter()
                        .find(|vp| vp.component_name == component.name && vp.pin_name == pin.name)
                        .and_then(|vp| vp.net.clone());

                    if let Some(net_name) = entity_graph_net {
                        // Use net assignment from net: block
                        net_names.push(net_name);
                    } else if let Some(net_id) = pin.connected_net {
                        // Fall back to netlist arena connection (from routing)
                        if let Some(net) = space.netlist.get_net(net_id) {
                            net_names.push(net.name.clone());
                        } else {
                            net_names.push(format!("node_{}", net_id.raw()).into());
                        }
                    } else {
                        // No connection - floating pin
                        net_names.push(format!("nc_{}", pin_id.raw()).into());
                    }
                }
            }

            // Emit SPICE device based on component type
            match component.component_type.to_uppercase().as_str() {
                "NMOS" | "PMOS" => {
                    // MOSFET: M<name> <drain> <gate> <source> <bulk> <model>
                    // Assume pin order: Gate, Source, Drain, Bulk
                    if net_names.len() >= 3 {
                        let gate = net_names.first().cloned().unwrap_or_else(|| "0".into());
                        let source = net_names.get(1).cloned().unwrap_or_else(|| "0".into());
                        let drain = net_names.get(2).cloned().unwrap_or_else(|| "0".into());
                        let bulk = net_names.get(3).cloned().unwrap_or_else(|| "0".into());

                        netlist_str.push_str(&format!(
                            "M{} {} {} {} {} {}\n",
                            component.name, drain, gate, source, bulk, component.component_type
                        ));
                    } else {
                        netlist_str.push_str(&format!(
                            "* WARNING: {} has insufficient pins for MOSFET\n",
                            component.name
                        ));
                    }
                }
                _ => {
                    // Generic subcircuit: X<name> <nets...> <type>
                    netlist_str.push_str(&format!(
                        "X{} {} {}\n",
                        component.name,
                        net_names.join(" "),
                        component.component_type
                    ));
                }
            }
        }
    }
    netlist_str.push('\n');
}
