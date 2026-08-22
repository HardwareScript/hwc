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
    /// This supports both first-class nominal device declarations and
    /// material-constrained type inference with ambiguity guards (Rustc E0282 pattern).
    pub fn from_pour_bindings(
        bindings: &FxHashMap<
            CompactString,
            FxHashMap<CompactString, Vec<hwc_engine::space::PourMetadata>>,
        >,
        symbol_table: &hwc_compiler::SymbolTable,
        space_def: Option<&hwc_parser::SpaceDefinition>,
    ) -> (Self, Vec<super::error::DeviceExtractionError>) {
        let mut extracted = Self::new();
        let mut errors = Vec::new();

        for (device_name, terminals_map) in bindings {
            let terminal_names: Vec<CompactString> = terminals_map.keys().cloned().collect();

            // 1. Check for first-class nominal declaration in space block
            let nominal = space_def.and_then(|sd| {
                sd.declared_devices
                    .iter()
                    .find(|d| d.instance_name == *device_name)
            });

            if let Some(decl) = nominal {
                if symbol_table.get_device(decl.device_type.as_str()).is_ok() {
                    extracted.devices.push((device_name.clone(), decl.device_type.clone()));
                    extracted
                        .device_terminals
                        .insert(device_name.clone(), terminal_names);
                    continue;
                }
            }

            // 2. Material-constrained inference with ambiguity guard
            match find_device_type_by_terminals_and_materials(
                device_name,
                &terminal_names,
                terminals_map,
                symbol_table,
            ) {
                Ok(device_type) => {
                    println!(
                        "      ├─ Discovered device '{}' of type '{}' from pour bindings (matched {} terminals)",
                        device_name, device_type, terminal_names.len()
                    );
                    extracted.devices.push((device_name.clone(), device_type));
                    extracted
                        .device_terminals
                        .insert(device_name.clone(), terminal_names);
                }
                Err(err) => {
                    errors.push(err);
                }
            }
        }

        (extracted, errors)
    }
}

impl Default for ExtractedDevices {
    fn default() -> Self {
        Self::new()
    }
}

/// Find device type by matching terminal names AND physical materials against PDK definitions
///
/// **Material-Constrained Inference with Ambiguity Guard (Rustc E0282 pattern)**
///
/// 1. Checks that all bound terminals exist in the candidate device definition
/// 2. Checks that physical pour materials match the candidate's allowed terminal materials
/// 3. Checks that any unbound terminals are virtual (e.g. Air/Vacuum)
/// 4. Fails fast with AmbiguousDeviceType if >1 candidate matches
fn find_device_type_by_terminals_and_materials(
    device_name: &CompactString,
    terminals: &[CompactString],
    terminals_map: &FxHashMap<CompactString, Vec<hwc_engine::space::PourMetadata>>,
    symbol_table: &hwc_compiler::SymbolTable,
) -> Result<CompactString, super::error::DeviceExtractionError> {
    let terminal_set: std::collections::HashSet<&str> =
        terminals.iter().map(|s| s.as_str()).collect();

    let mut matching_candidates = Vec::new();

    // Iterate through all device definitions in the symbol table
    for (cand_name, device_def) in symbol_table.iter_all_devices() {
        // Check if all bound terminals exist in the device definition
        let all_bound_terminals_valid = terminal_set.iter().all(|t| {
            device_def.terminals.iter().any(|def_t| def_t.as_str() == *t)
        });

        if !all_bound_terminals_valid {
            continue;
        }

        // Check physical materials of all bound pours
        let mut materials_compatible = true;
        for (term_name, pours) in terminals_map {
            for pour in pours {
                if !device_def.is_material_allowed(term_name.as_str(), pour.material_name.as_str()) {
                    materials_compatible = false;
                    break;
                }
            }
            if !materials_compatible {
                break;
            }
        }

        if !materials_compatible {
            continue;
        }

        // Check if all missing terminals are virtual (material: Air/Vacuum)
        let missing_terminals: Vec<&CompactString> = device_def
            .terminals
            .iter()
            .filter(|t| !terminal_set.contains(t.as_str()))
            .collect();

        let all_missing_are_virtual = missing_terminals.iter().all(|terminal| {
            if let Some(allowed_materials) = device_def.materials.get(*terminal) {
                allowed_materials.iter().any(|mat| mat.as_str() == "Air" || mat.as_str() == "Vacuum")
            } else {
                false
            }
        });

        if all_missing_are_virtual {
            matching_candidates.push(cand_name.clone());
        }
    }

    match matching_candidates.len() {
        1 => Ok(matching_candidates.remove(0)),
        0 => Err(super::error::DeviceExtractionError::NoMatchingContract {
            instance: device_name.clone(),
            details: format!(
                "Physical layout materials/terminals for instance '{}' do not satisfy any registered PDK device contract.",
                device_name
            ),
        }),
        _ => Err(super::error::DeviceExtractionError::AmbiguousDeviceType {
            instance: device_name.clone(),
            candidates: matching_candidates,
            hint: format!("Explicitly declare device type: 'device <Type> named {}'", device_name),
        }),
    }
}
