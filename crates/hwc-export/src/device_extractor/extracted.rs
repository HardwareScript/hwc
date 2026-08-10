use compact_str::CompactString;
use hwc_parser::ast::{AstArena, ModuleComponentPlacement, ModuleDefinition};
use rustc_hash::FxHashMap;

/// Extracted device information from module statements
#[derive(Debug, Clone)]
pub struct ExtractedDevices {
    /// List of devices with their types: (device_name, device_type)
    pub devices: Vec<(CompactString, CompactString)>,
    /// Map of device terminals: device_name -> [terminal_names]
    pub device_terminals: FxHashMap<CompactString, Vec<CompactString>>,
}

impl ExtractedDevices {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            device_terminals: FxHashMap::default(),
        }
    }

    /// Extract devices and their terminals from module statements
    ///
    /// Parses the module to find:
    /// 1. Which devices exist (from `add` statements OR device bindings)
    /// 2. Which terminals each device uses (from `route` statements OR bindings)
    pub fn from_module(module: &ModuleDefinition, arena: &AstArena) -> Self {
        use hwc_parser::ast::ModuleStatement;

        let mut extracted = Self::new();

        for statement in &module.statements {
            match statement {
                ModuleStatement::AddComponent(add_id) => {
                    // Look up the actual component placement from the arena
                    let add: &ModuleComponentPlacement = &arena.module_components[*add_id];
                    if let Some(ref instance_name) = add.name {
                        extracted
                            .devices
                            .push((instance_name.clone(), add.component_type.clone()));
                    }
                }
                ModuleStatement::Route(route) => {
                    // Parse "DeviceName.terminal" format from the 'from' pin reference
                    let device = &route.from.component;
                    let terminal = &route.from.pin;

                    extracted
                        .device_terminals
                        .entry(device.clone())
                        .or_default()
                        .push(terminal.clone());
                }
                _ => {}
            }
        }

        extracted
    }

    /// Extract devices from pour device bindings
    ///
    /// Scans pour bindings to discover which devices exist and their terminals.
    /// This supports the native `device` keyword pattern where pours are bound
    /// to device terminals using `device: DeviceName.terminal`.
    ///
    /// Queries the symbol table to find the correct device type by matching terminal names.
    pub fn from_pour_bindings(
        bindings: &FxHashMap<
            CompactString,
            FxHashMap<CompactString, Vec<hwc_engine::space::PourMetadata>>,
        >,
        symbol_table: &hwc_compiler::SymbolTable,
    ) -> Self {
        let mut extracted = Self::new();

        for (device_name, terminals_map) in bindings {
            let terminal_names: Vec<CompactString> = terminals_map.keys().cloned().collect();

            // Query symbol table to find which device definition matches these terminals
            match find_device_type_by_terminals(&terminal_names, symbol_table) {
                Some(device_type) => {
                    println!("      ├─ Discovered device '{}' of type '{}' from pour bindings (matched {} terminals)", 
                             device_name, device_type, terminal_names.len());

                    extracted.devices.push((device_name.clone(), device_type));
                    extracted
                        .device_terminals
                        .insert(device_name.clone(), terminal_names);
                }
                None => {
                    // No matching device definition found - this is an error
                    println!("      ├─ ERROR: Device '{}' has terminals {:?} that don't match any device definition in symbol table", 
                             device_name, terminal_names);
                    // Don't add to extracted devices - will fail with clear error later
                }
            }
        }

        extracted
    }
}

impl Default for ExtractedDevices {
    fn default() -> Self {
        Self::new()
    }
}

/// Find device type by matching terminal names against device definitions in symbol table
///
/// **v0.2.2: Partial Matching with Virtual Terminal Support**
///
/// This performs a **subset match**: the terminal names from pour bindings must be a subset
/// of the terminals defined in a device definition. Missing terminals are allowed if they
/// are virtual terminals (material: Air).
///
/// This allows devices with virtual terminals (like BULK, SUBSTRATE) to be discovered even
/// when only physical terminals (A, B, GATE, etc.) have pour bindings.
///
/// Returns the device type name if a match is found, None otherwise.
fn find_device_type_by_terminals(
    terminals: &[CompactString],
    symbol_table: &hwc_compiler::SymbolTable,
) -> Option<CompactString> {
    // Convert terminals to a set for fast lookup
    let terminal_set: std::collections::HashSet<&str> = 
        terminals.iter().map(|s| s.as_str()).collect();

    // Iterate through all device definitions in the symbol table
    for (device_name, device_def) in symbol_table.iter_all_devices() {
        // Check if all bound terminals exist in the device definition
        let all_bound_terminals_valid = terminal_set.iter().all(|t| {
            device_def.terminals.iter().any(|def_t| def_t.as_str() == *t)
        });

        if !all_bound_terminals_valid {
            // Some bound terminals don't exist in this device definition - skip
            continue;
        }

        // Check if all missing terminals are virtual (material: Air)
        let missing_terminals: Vec<&CompactString> = device_def
            .terminals
            .iter()
            .filter(|t| !terminal_set.contains(t.as_str()))
            .collect();

        // All missing terminals must be virtual (allowed material: Air)
        let all_missing_are_virtual = missing_terminals.iter().all(|terminal| {
            if let Some(allowed_materials) = device_def.materials.get(*terminal) {
                allowed_materials.iter().any(|mat| mat.as_str() == "Air" || mat.as_str() == "Vacuum")
            } else {
                false // No material constraint = physical terminal required
            }
        });

        if all_missing_are_virtual {
            return Some(device_name.clone());
        }
    }

    // No matching device definition found
    None
}
