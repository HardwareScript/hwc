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
    /// 1. Which devices exist (from `add` statements)
    /// 2. Which terminals each device uses (from `route` statements)
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
}

impl Default for ExtractedDevices {
    fn default() -> Self {
        Self::new()
    }
}
