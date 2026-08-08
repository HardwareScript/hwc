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
            FxHashMap<CompactString, hwc_engine::space::PourMetadata>,
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
/// This performs a precise match: the terminal names from pour bindings must exactly match
/// the terminals defined in a device definition (order-independent).
///
/// Returns the device type name if a match is found, None otherwise.
fn find_device_type_by_terminals(
    terminals: &[CompactString],
    symbol_table: &hwc_compiler::SymbolTable,
) -> Option<CompactString> {
    // Convert terminals to a sorted set for order-independent comparison
    let mut terminal_set: Vec<&str> = terminals.iter().map(|s| s.as_str()).collect();
    terminal_set.sort_unstable();

    // Iterate through all device definitions in the symbol table
    for (device_name, device_def) in symbol_table.iter_all_devices() {
        // Get terminals from device definition and sort them
        let mut def_terminals: Vec<&str> =
            device_def.terminals.iter().map(|t| t.as_str()).collect();
        def_terminals.sort_unstable();

        // If terminals match exactly, we found the device type
        if terminal_set == def_terminals {
            return Some(device_name.clone());
        }
    }

    // No matching device definition found
    None
}
