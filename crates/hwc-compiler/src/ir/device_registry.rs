//! Device Registry Population (v0.2.1)
//!
//! Populates the HardwareSpace.device_instances registry during compilation.
//! This is the PROPER ARCHITECTURE - devices are discovered and stored once
//! during compilation, then used by all export formats without re-inference.

use compact_str::CompactString;
use hwc_engine::space::{DeviceInstance, PourMetadata};
use rustc_hash::FxHashMap;

use crate::{IrError, SymbolTable};

/// Populate device instances in HardwareSpace from pour bindings
///
/// This scans all pours with device_binding and groups them by device instance,
/// then looks up the device definition from the symbol table to match terminals and materials.
pub fn populate_device_instances(
    space: &mut hwc_engine::HardwareSpace,
    symbol_table: &SymbolTable,
) -> Result<(), IrError> {
    println!("   ├─ Populating device instance registry...");

    // Step 1: Group pours by device instance name
    let mut device_groups: FxHashMap<CompactString, Vec<&PourMetadata>> = FxHashMap::default();

    for pour in &space.pours {
        if let Some(ref binding) = pour.device_binding {
            device_groups
                .entry(binding.device_name.clone())
                .or_default()
                .push(pour);
        }
    }

    // Step 2: For each device instance, create DeviceInstance metadata
    for (device_name, pours) in device_groups {
        // Extract terminal names and materials from bindings
        let mut terminals = Vec::new();
        let mut terminal_nets: FxHashMap<CompactString, CompactString> = FxHashMap::default();
        let mut terminal_materials: FxHashMap<CompactString, CompactString> = FxHashMap::default();

        for pour in &pours {
            if let Some(ref binding) = pour.device_binding {
                if !terminals.contains(&binding.terminal) {
                    terminals.push(binding.terminal.clone());
                }

                // ZERO COMPILER MAGIC: Device terminal pours MUST have explicit net assignments
                // HardwareScript does not infer connectivity - the user must declare it explicitly
                if pour.net.is_none() {
                    return Err(IrError::DeviceTerminalMissingNet {
                        pour_name: pour.name.clone(),
                        device: binding.device_name.clone(),
                        terminal: binding.terminal.clone(),
                        material: pour.material_name.clone(),
                    });
                }

                // Map terminal to net
                if let Some(ref net) = pour.net {
                    terminal_nets.insert(binding.terminal.clone(), net.clone());
                }

                // Map terminal to material (use first pour's material for each terminal)
                terminal_materials
                    .entry(binding.terminal.clone())
                    .or_insert_with(|| pour.material_name.clone());
            }
        }

        // Look up device type from symbol table by matching terminals and materials
        let device_type = match lookup_device_type_from_symbol_table(
            symbol_table,
            &terminals,
            &terminal_materials,
        ) {
            Some(name) => name,
            None => {
                println!("      ⚠ Warning: Could not find device definition matching terminals {:?} with materials {:?}", 
                         terminals, terminal_materials);
                "UnknownDevice".into()
            }
        };

        // Calculate parameters based on geometry and device type
        let parameters = calculate_device_parameters(&device_type, &pours);

        let device_instance = DeviceInstance {
            name: device_name.clone(),
            device_type,
            terminals,
            terminal_nets,
            parameters,
        };

        println!(
            "      ├─ Registered device '{}' of type '{}' with {} terminals",
            device_instance.name,
            device_instance.device_type,
            device_instance.terminals.len()
        );

        space.device_instances.push(device_instance);
    }

    println!(
        "   ├─ Device registry populated: {} devices",
        space.device_instances.len()
    );
    Ok(())
}

/// Look up device type from symbol table by matching terminals and materials
///
/// This searches all device definitions in the symbol table and finds the one
/// that matches the observed terminals and materials from the pours.
fn lookup_device_type_from_symbol_table(
    symbol_table: &SymbolTable,
    terminals: &[CompactString],
    terminal_materials: &FxHashMap<CompactString, CompactString>,
) -> Option<CompactString> {
    // Iterate through all device definitions in priority order
    for (name, device_def) in symbol_table.iter_all_devices() {
        // Check if all observed terminals exist in the device definition
        let all_terminals_match = terminals
            .iter()
            .all(|terminal| device_def.has_terminal(terminal.as_str()));

        if !all_terminals_match {
            continue;
        }

        // Check if materials match the device definition's requirements
        let all_materials_match = terminal_materials.iter().all(|(terminal, material)| {
            device_def.is_material_allowed(terminal.as_str(), material.as_str())
        });

        if all_materials_match {
            return Some(name.clone());
        }
    }

    None
}

/// Calculate device parameters from geometry
fn calculate_device_parameters(
    device_type: &str,
    pours: &[&PourMetadata],
) -> FxHashMap<CompactString, f64> {
    let mut params = FxHashMap::default();

    match device_type {
        "Resistor" => {
            // Find the resistive body pour (usually the largest)
            if let Some(body_pour) = pours.iter().max_by_key(|p| p.area_nm2) {
                // Calculate resistance: R = R_sheet * (L/W)
                // For now, assume square geometry
                let area_nm2 = body_pour.area_nm2 as f64;
                let side_nm = area_nm2.sqrt();
                let side_um = side_nm / 1000.0;

                // Sheet resistance for polysilicon: ~400 Ohms/square
                let sheet_resistance = 400.0;

                // For a square, L/W = 1, so R = R_sheet
                // For rectangles, would need actual L and W
                let resistance = sheet_resistance; // Simplified for now

                params.insert("R".into(), resistance);
                params.insert("W".into(), side_um);
                params.insert("L".into(), side_um);
            }
        }
        "NMOS" | "PMOS" => {
            // Find gate pour to calculate W/L
            if let Some(gate_pour) = pours
                .iter()
                .find(|p| p.device_binding.as_ref().map(|b| b.terminal.as_str()) == Some("gate"))
            {
                let area_nm2 = gate_pour.area_nm2 as f64;
                let side_nm = area_nm2.sqrt();
                let side_um = side_nm / 1000.0;

                params.insert("W".into(), side_um);
                params.insert("L".into(), side_um);
            }
        }
        "Capacitor" => {
            // Calculate capacitance from area and dielectric thickness
            // C = ε₀ε_r(A/d)
            if let Some(plate_pour) = pours.first() {
                let area_m2 = (plate_pour.area_nm2 as f64) / 1e18;
                let epsilon_0 = 8.854e-12; // F/m
                let epsilon_r = 3.9; // SiO2
                let thickness_m = 10e-9; // 10nm typical

                let capacitance = epsilon_0 * epsilon_r * area_m2 / thickness_m;
                params.insert("C".into(), capacitance);
            }
        }
        _ => {}
    }

    params
}

/// Convert device_instances from HardwareSpace to PhysicalNetlist for export
///
/// This bridges the gap between the compiler's device registry (space.device_instances)
/// and the alignment/export layer's PhysicalNetlist format.
///
/// # Arguments
/// * `space` - The hardware space containing device instances
/// * `space_def` - Optional space definition (needed to map module ports)
/// * `symbol_table` - Optional symbol table (needed to look up module definition)
pub fn device_instances_to_physical_netlist(
    space: &hwc_engine::HardwareSpace,
    space_def: Option<&hwc_parser::SpaceDefinition>,
    symbol_table: Option<&crate::SymbolTable>,
) -> crate::alignment::PhysicalNetlist {
    use crate::alignment::{
        DeviceTypeRegistry, NetInfo, PhysicalDevice, PhysicalNetlist, PortDirection, PortInfo,
    };

    let mut device_registry = DeviceTypeRegistry::new();
    let mut physical_netlist = PhysicalNetlist::with_registry(device_registry.clone());

    // Build a map of device.terminal -> pour_name for terminal_pours field
    let mut device_terminal_pours: rustc_hash::FxHashMap<(CompactString, CompactString), String> =
        rustc_hash::FxHashMap::default();

    for pour in &space.pours {
        if let Some(ref binding) = pour.device_binding {
            let key = (binding.device_name.clone(), binding.terminal.clone());
            device_terminal_pours.insert(key, pour.name.to_string());
        }
    }

    // Convert each device instance to a PhysicalDevice
    for device_instance in &space.device_instances {
        // Register device type and get ID
        let device_type_id = device_registry.get_or_register(&device_instance.device_type);

        // Convert terminal_nets from FxHashMap<CompactString, CompactString> to FxHashMap<CompactString, String>
        let terminals: rustc_hash::FxHashMap<CompactString, String> = device_instance
            .terminal_nets
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();

        // Build terminal_pours map
        let terminal_pours: rustc_hash::FxHashMap<CompactString, String> = device_instance
            .terminals
            .iter()
            .filter_map(|terminal| {
                let key = (device_instance.name.clone(), terminal.clone());
                device_terminal_pours
                    .get(&key)
                    .map(|pour_name| (terminal.clone(), pour_name.clone()))
            })
            .collect();

        let physical_device = PhysicalDevice {
            name: device_instance.name.clone(),
            device_type_id,
            terminals,
            parameters: device_instance.parameters.clone(),
            terminal_pours,
        };

        physical_netlist.devices.push(physical_device);

        // Register nets from terminal connections
        for net_name in device_instance.terminal_nets.values() {
            physical_netlist
                .nets
                .entry(net_name.clone())
                .or_insert_with(|| NetInfo {
                    name: net_name.clone(),
                    connected_devices: Vec::new(),
                });

            // Add device name to net's connected devices list
            if let Some(net_info) = physical_netlist.nets.get_mut(net_name) {
                if !net_info.connected_devices.contains(&device_instance.name) {
                    net_info
                        .connected_devices
                        .push(device_instance.name.clone());
                }
            }
        }
    }

    // Add all nets from the space's netlist as well (including ports/external connections)
    for net_id in space.netlist.all_net_ids() {
        if let Some(net) = space.netlist.get_net(net_id) {
            physical_netlist
                .nets
                .entry(net.name.clone())
                .or_insert_with(|| NetInfo {
                    name: net.name.clone(),
                    connected_devices: Vec::new(),
                });
        }
    }

    // Update the registry in the netlist
    physical_netlist.device_registry = device_registry;

    // Map module ports to physical netlist ports
    if let Some(space_def) = space_def {
        if let Some(module_name) = &space_def.implements_module {
            if let Some(symbol_table) = symbol_table {
                if let Ok(module_def) = symbol_table.get_module(module_name) {
                    // For each pin in the module, create a corresponding port in the physical netlist
                    for pin in &module_def.pins {
                        // Convert AST PinDirection to alignment PortDirection
                        let direction = match pin.direction {
                            hwc_parser::PinDirection::Input => PortDirection::Input,
                            hwc_parser::PinDirection::Output => PortDirection::Output,
                            hwc_parser::PinDirection::Inout => PortDirection::Inout,
                            hwc_parser::PinDirection::Power => PortDirection::Power,
                            hwc_parser::PinDirection::Ground => PortDirection::Ground,
                            hwc_parser::PinDirection::Passive => PortDirection::Inout,
                        };

                        physical_netlist.ports.push(PortInfo {
                            name: pin.name.clone(),
                            direction,
                        });
                    }
                }
            }
        }
    }

    physical_netlist
}
