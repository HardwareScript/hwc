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
    space_def: Option<&hwc_parser::ast::SpaceDefinition>,
) -> Result<(), IrError> {
    println!("   â”œâ”€ Populating device instance registry...");

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

        // v0.2.2: Sort pours by binding priority (declarative, not heuristic)
        // Channel pours (priority=0) processed first, Contact pours (priority=100) last to override
        let mut sorted_pours = pours.clone();
        sorted_pours.sort_by_key(|p| {
            p.device_binding
                .as_ref()
                .map_or(hwc_engine::space::BindingPriority::default(), |b| b.priority)
        });

        println!("      â”œâ”€ Device '{}': Processing {} pours in priority order:", device_name, sorted_pours.len());
        for pour in &sorted_pours {
            if let Some(ref binding) = pour.device_binding {
                println!("      â”‚  Pour '{}': priority={:?}, terminals={:?}, net={:?}", 
                    pour.name, binding.priority, binding.terminals, pour.net);
            }
        }

        for pour in &sorted_pours {
            if let Some(ref binding) = pour.device_binding {
                // v0.2.2: Handle multi-terminal bindings
                for terminal in &binding.terminals {
                    if !terminals.contains(terminal) {
                        terminals.push(terminal.clone());
                    }

                    // ZERO COMPILER MAGIC: Device terminal pours MUST have explicit net assignments
                    // HardwareScript does not infer connectivity - the user must declare it explicitly
                    // 
                    // v0.2.2 EXEMPTION: Multi-terminal device bodies (e.g., resistor channel) are exempt.
                    // These pours span multiple terminals and cannot belong to a single net.
                    if pour.net.is_none() && binding.terminals.len() == 1 {
                        return Err(IrError::DeviceTerminalMissingNet {
                            pour_name: pour.name.clone(),
                            device: binding.device_name.clone(),
                            terminal: terminal.clone(),
                            material: pour.material_name.clone(),
                        });
                    }

                    // Map terminal to net with declarative priority:
                    // - Contact pours (priority=100): Always override
                    // - Channel pours (priority=0): Only insert if not already mapped
                    if let Some(ref net) = pour.net {
                        let before = terminal_nets.get(terminal).cloned();
                        if binding.priority == hwc_engine::space::BindingPriority::Contact {
                            // Contact pour: Always set the net (contact heads have priority)
                            terminal_nets.insert(terminal.clone(), net.clone());
                            println!("      â”‚  Terminal '{}': Contact pour '{}' set net {:?} -> {:?}", 
                                terminal, pour.name, before, net);
                        } else {
                            // Channel pour: Only set if not already defined by a contact
                            terminal_nets.entry(terminal.clone()).or_insert(net.clone());
                            println!("      â”‚  Terminal '{}': Channel pour '{}' set net {:?} -> {:?} (if not already set)", 
                                terminal, pour.name, before, terminal_nets.get(terminal));
                        }
                    }

                    // Map terminal to material (use first pour's material for each terminal)
                    terminal_materials
                        .entry(terminal.clone())
                        .or_insert_with(|| pour.material_name.clone());
                }
            }
        }

        // Step 2b: Add virtual terminals from space_def.device_nets (v0.2.1)
        // Virtual terminals (material: Air) don't require physical geometry
        // but MUST have explicit net mappings via device_nets declarations
        if let Some(space_def) = space_def {
            if let Some(device_nets) = space_def.device_nets.get(&device_name) {
                for (terminal_name, net_name) in device_nets {
                    // Only add if not already bound (virtual terminals have no pours)
                    if !terminals.contains(terminal_name) {
                        terminals.push(terminal_name.clone());
                        terminal_nets.insert(terminal_name.clone(), net_name.clone());
                        terminal_materials.insert(terminal_name.clone(), "Air".into());
                        
                        println!(
                            "      â”œâ”€ Device '{}' virtual terminal '{}' â†’ net '{}'",
                            device_name, terminal_name, net_name
                        );
                    }
                }
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
                // Try to find close matches for better error message
                let available_devices = find_similar_devices(symbol_table, &terminals, &terminal_materials);
                
                let mut error_msg = format!(
                    "Device '{}' bindings do not match any device definition in the symbol table.\n\
                     \n\
                     Device bindings found in layout:\n\
                     - Terminals: {:?}\n\
                     - Materials: {:?}\n",
                    device_name, terminals, terminal_materials
                );

                if !available_devices.is_empty() {
                    error_msg.push_str("\nPossible matches with terminal/material mismatches:\n");
                    for (dev_name, mismatch) in available_devices {
                        error_msg.push_str(&format!("  - device '{}': {}\n", dev_name, mismatch));
                    }
                } else {
                    error_msg.push_str("\nNo device definitions found with matching terminals.\n");
                }

                error_msg.push_str(
                    "\nTo fix this:\n\
                     1. Check that you've imported the device definition (import MyDevice from \"./pdk\")\n\
                     2. Verify the device contract materials match your layout's pour materials\n\
                     3. Ensure all terminals in the device definition are bound to pours with the correct materials\n"
                );

                return Err(IrError::DeviceRegistryError { message: error_msg });
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
            "      â”œâ”€ Registered device '{}' of type '{}' with {} terminals",
            device_instance.name,
            device_instance.device_type,
            device_instance.terminals.len()
        );
        println!(
            "      â”‚  Terminal net mappings: {:?}",
            device_instance.terminal_nets
        );

        space.device_instances.push(device_instance);
    }

    println!(
        "   â”œâ”€ Device registry populated: {} devices",
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

/// Find devices with similar terminals but mismatched materials to provide helpful error messages
fn find_similar_devices(
    symbol_table: &SymbolTable,
    terminals: &[CompactString],
    terminal_materials: &FxHashMap<CompactString, CompactString>,
) -> Vec<(CompactString, String)> {
    let mut similar = Vec::new();

    for (name, device_def) in symbol_table.iter_all_devices() {
        // Check if terminals match
        let all_terminals_match = terminals
            .iter()
            .all(|terminal| device_def.has_terminal(terminal.as_str()));

        if all_terminals_match {
            // Terminals match but materials don't - find which materials mismatch
            let mut mismatches = Vec::new();
            for (terminal, actual_material) in terminal_materials.iter() {
                if !device_def.is_material_allowed(terminal.as_str(), actual_material.as_str()) {
                    // Get expected materials
                    if let Some(expected_materials) = device_def.get_terminal_materials(terminal.as_str()) {
                        let expected_str = if expected_materials.len() == 1 {
                            format!("'{}'", expected_materials[0])
                        } else {
                            format!("one of [{}]", expected_materials.iter().map(|m| format!("'{}'", m)).collect::<Vec<_>>().join(", "))
                        };
                        mismatches.push(format!(
                            "terminal '{}' expects {} but layout uses '{}'",
                            terminal, expected_str, actual_material
                        ));
                    }
                }
            }
            
            if !mismatches.is_empty() {
                similar.push((name.clone(), mismatches.join(", ")));
            }
        }
    }

    similar
}

/// Calculate device parameters from geometry
///
/// This is a stub that will be replaced by proper parameter extraction during export.
/// Device parameters should NOT be calculated during IR compilation because:
/// 1. The compiler doesn't have access to material properties or stackup
/// 2. Parameter extraction is an export-time concern (SPICE needs it, DXF doesn't)
/// 3. This creates a layering violation where IR knows about physics
///
/// Instead, this returns empty parameters and lets the export modules calculate
/// them using the proper ParameterExtractionRegistry with full material/stackup context.
fn calculate_device_parameters(
    _device_type: &str,
    _pours: &[&PourMetadata],
) -> FxHashMap<CompactString, f64> {
    // Return empty parameters - extraction happens at export time
    // This maintains proper architectural layering:
    // - IR layer: knows about structure (devices, terminals, bindings)
    // - Export layer: knows about physics (R, C, W, L calculations)
    FxHashMap::default()
}

/// Extract device parameters from space geometry using generic lookup tables
///
/// This function uses the device definition from the symbol table to determine
/// which parameters need to be extracted, then calculates them from the geometry.
/// NO HARDCODING - all extraction logic is driven by the device definition.
fn extract_device_parameters_from_space(
    device_type: &str,
    _device_name: &CompactString,
    _terminals: &[CompactString],
    _space: &hwc_engine::HardwareSpace,
    symbol_table: Option<&crate::SymbolTable>,
) -> FxHashMap<CompactString, hwc_engine::PhysicalQuantity> {
    let parameters = FxHashMap::default();

    // Get device definition to see what parameters are needed
    let Some(symbol_table) = symbol_table else {
//         eprintln!("[PARAM EXTRACTION] No symbol table provided for device '{}'", device_name);
        return parameters;
    };

    let device_def = match symbol_table.get_device(device_type) {
        Ok(def) => def,
        Err(_) => {
//             eprintln!("[PARAM EXTRACTION] Device type '{}' not found in symbol table", device_type);
            return parameters;
        }
    };

    // Get the SPICE parameters list from the device definition
    let spice_params = &device_def.spice_info.as_ref().map(|s| &s.parameters);
    let Some(_param_names) = spice_params else {
//         eprintln!("[PARAM EXTRACTION] Device '{}' has no SPICE parameters defined", device_type);
        return parameters;
    };

    // ============================================================================
    // DEPRECATED: Legacy hardcoded parameter extraction (v0.2.1)
    // ============================================================================
    // This code path has been DISABLED in favor of the registry-based extraction
    // system in hwc-export/src/device_extractor/parameter_extraction.rs
    //
    // The legacy system had critical flaws:
    // 1. Measured width as "minimum across all pours" â†’ extracted contact pad width (400nm)
    //    instead of resistor body width (1Î¼m)
    // 2. No material awareness â†’ couldn't distinguish resistive channel from contacts
    // 3. Silent success with wrong data â†’ no error detection for missing geometry
    //
    // The new registry-based system:
    // - Filters pours by material properties (resistive vs contact materials)
    // - Fails loudly with error[D03] when primary channel geometry is missing
    // - Extensible via ParameterExtractionRegistry for device-specific rules
    //
    // TODO: Remove this entire function once migration is complete (v0.3.0)
    // ============================================================================
    
//     eprintln!(
//         "[PARAM EXTRACTION] Legacy hardcoded extraction DISABLED for device '{}'. \
//          Parameters will be extracted by the registry-based system.",
//         device_name
//     );

    parameters
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

    // v0.2.2: Sort pours by binding priority (declarative, not heuristic)
    // Channel pours (priority=0) are processed first, Contact pours (priority=100) last to override
    let mut sorted_pours = space.pours.iter().collect::<Vec<_>>();
    sorted_pours.sort_by_key(|p| {
        p.device_binding
            .as_ref()
            .map_or(hwc_engine::space::BindingPriority::default(), |b| b.priority)
    });

    for pour in &sorted_pours {
        if let Some(ref binding) = pour.device_binding {
            // v0.2.2: Handle multi-terminal bindings - create entry for each terminal
            for terminal in &binding.terminals {
                let key = (binding.device_name.clone(), terminal.clone());
                device_terminal_pours.insert(key, pour.name.to_string());
            }
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
        
        eprintln!(
            "[PHYSICAL NETLIST DEBUG] Device '{}': Converting terminal_nets {:?} to terminals {:?}",
            device_instance.name, device_instance.terminal_nets, terminals
        );

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

        // Extract parameters from geometry using the registry-based system
        // This uses the generic extraction functions registered for each device type
        let parameters = extract_device_parameters_from_space(
            &device_instance.device_type,
            &device_instance.name,
            &device_instance.terminals,
            space,
            symbol_table,
        );

        let physical_device = PhysicalDevice {
            name: device_instance.name.clone(),
            device_type_id,
            terminals,
            parameters,
            terminal_pours,
        };

        eprintln!(
            "[PHYSICAL NETLIST DEBUG] Created PhysicalDevice '{}': terminals = {:?}",
            physical_device.name, physical_device.terminals
        );

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
