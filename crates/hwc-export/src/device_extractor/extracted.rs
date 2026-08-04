use compact_str::CompactString;
use hwc_parser::ast::ModuleDefinition;
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
    pub fn from_module(module: &ModuleDefinition) -> Self {
        use hwc_parser::ast::ModuleStatement;

        let mut extracted = Self::new();

        for statement in &module.statements {
            match statement {
                ModuleStatement::AddComponent(add) => {
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
    /// Since we don't have explicit type information from the bindings alone,
    /// we infer the device type from the defined device contracts in the file.
    pub fn from_pour_bindings(
        bindings: &FxHashMap<CompactString, FxHashMap<CompactString, hwc_engine::space::PourMetadata>>,
    ) -> Self {
        let mut extracted = Self::new();

        for (device_name, terminals_map) in bindings {
            // For now, assume all device instances with 2-terminal bindings (A, B) are Resistors
            // This is a temporary heuristic until we have explicit device type declarations
            let terminal_names: Vec<CompactString> = terminals_map.keys().cloned().collect();
            
            let device_type: CompactString = if terminal_names.len() == 2 
                && terminal_names.contains(&"A".into()) 
                && terminal_names.contains(&"B".into()) {
                "Resistor".into()
            } else if terminal_names.len() == 4 
                && terminal_names.contains(&"gate".into()) 
                && terminal_names.contains(&"source".into())
                && terminal_names.contains(&"drain".into())
                && terminal_names.contains(&"bulk".into()) {
                "NMOS".into() // or PMOS - would need additional logic
            } else {
                // Generic fallback
                "Device".into()
            };
            
            extracted.devices.push((device_name.clone(), device_type.clone()));
            extracted.device_terminals.insert(device_name.clone(), terminal_names);
            
            println!("      ├─ Discovered device '{}' of type '{}' from pour bindings", device_name, device_type);
        }

        extracted
    }
}

impl Default for ExtractedDevices {
    fn default() -> Self {
        Self::new()
    }
}
