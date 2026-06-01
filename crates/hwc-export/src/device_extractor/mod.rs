//! Device Extractor - Phase 4: Silent Atom Architecture
//!
//! This module implements EXPLICIT INTENT-BASED device extraction.

pub mod error;
pub mod extracted;
pub mod geometry;
pub mod mapping;
pub mod spice;
pub mod validation;

pub use error::DeviceExtractionError;
pub use extracted::ExtractedDevices;
pub use spice::format_spice;

use compact_str::CompactString;
use hwc_compiler::alignment::netlist::{
    DeviceTypeRegistry, NetInfo, PhysicalDevice, PhysicalNetlist,
};
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

/// Device extractor using explicit intent-based bindings
pub struct DeviceExtractor<'a> {
    pub(super) space: &'a HardwareSpace,
    pub(super) symbol_table: &'a hwc_compiler::SymbolTable,
    pub(super) device_registry: DeviceTypeRegistry,
    pub(super) material_database: hwc_materials::MaterialDatabase,
}

impl<'a> DeviceExtractor<'a> {
    /// Create a new device extractor
    pub fn new(space: &'a HardwareSpace, symbol_table: &'a hwc_compiler::SymbolTable) -> Self {
        // Populate material database from symbol table for physics validation
        let material_database = hwc_compiler::populate_material_database(symbol_table)
            .unwrap_or_else(|_| hwc_materials::MaterialDatabase::empty());

        Self {
            space,
            symbol_table,
            device_registry: DeviceTypeRegistry::new(),
            material_database,
        }
    }

    /// Extract devices using explicit intent-based bindings (Phase 4: Silent Atom)
    pub fn extract_devices_with_module(
        &mut self,
        module: Option<&hwc_parser::ast::ModuleDefinition>,
    ) -> Result<PhysicalNetlist, Vec<DeviceExtractionError>> {
        let module_def = module.ok_or_else(|| {
            vec![DeviceExtractionError::InvalidGeometry {
                device_name: "N/A".into(),
                device_type: "N/A".into(),
                reason: "Module definition required for device extraction. \
                        Geometric guessing is no longer supported. \
                        Use 'device: DeviceName.terminal' to bind pours to devices."
                    .into(),
            }]
        })?;

        self.extract_devices_intent_based(module_def)
    }

    /// Extract devices using explicit intent (Silent Atom core)
    fn extract_devices_intent_based(
        &mut self,
        module: &hwc_parser::ast::ModuleDefinition,
    ) -> Result<PhysicalNetlist, Vec<DeviceExtractionError>> {
        let mut netlist = PhysicalNetlist::new();
        let mut errors = Vec::new();

        // Step 1: Group all pours by their device binding
        let bindings = self.group_pours_by_device_binding();

        // Step 2: Extract devices and their terminals from module statements
        let extracted = ExtractedDevices::from_module(module);

        // Step 2.5: Build terminal-to-net mapping from module route statements
        let (terminal_to_net, all_net_names) = self.build_terminal_to_net_mapping(module);

        // Step 3: For each logical device, verify all terminals are bound
        for (device_name, device_type) in extracted.devices {
            println!("   ├─ Checking device: {} ({})", device_name, device_type);

            // Get all pours bound to this device
            let device_pours = match bindings.get(&device_name) {
                Some(pours) => pours.clone(),
                None => {
                    errors.push(DeviceExtractionError::InvalidGeometry {
                        device_name: device_name.clone(),
                        device_type: device_type.clone(),
                        reason: format!(
                            "No geometry bound to device '{}'. Use 'device: {}.terminal' to bind pours.",
                            device_name, device_name
                        ).into(),
                    });
                    continue;
                }
            };

            // Step 4: Verify all required terminals are bound
            let required_terminals = extracted
                .device_terminals
                .get(&device_name)
                .cloned()
                .unwrap_or_default();
            for terminal in &required_terminals {
                if !device_pours.contains_key(terminal) {
                    errors.push(DeviceExtractionError::InvalidGeometry {
                        device_name: device_name.clone(),
                        device_type: device_type.clone(),
                        reason: format!(
                            "Missing terminal binding: {}.{} not found. Add 'device: {}.{}' to a pour.",
                            device_name, terminal, device_name, terminal
                        ).into(),
                    });
                }
            }

            if !errors.is_empty() {
                continue;
            }

            // Step 5: Extract device from bound geometry
            match self.extract_bound_device(
                &device_name,
                &device_type,
                &device_pours,
                &terminal_to_net,
                &mut netlist,
            ) {
                Ok(()) => {
                    println!("      └─ Device extracted successfully ✓");
                }
                Err(e) => errors.push(e),
            }
        }

        if errors.is_empty() {
            for net_name in all_net_names {
                netlist
                    .nets
                    .entry(net_name.clone())
                    .or_insert_with(|| NetInfo::new(&net_name));
            }

            // Copy port information from module
            self.copy_ports_from_module(module, &mut netlist);

            // Update the netlist's device registry with all registered types
            netlist.device_registry = self.device_registry.clone();

            Ok(netlist)
        } else {
            Err(errors)
        }
    }

    /// Extract a device from explicitly bound geometry
    fn extract_bound_device(
        &mut self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, hwc_engine::space::PourMetadata>,
        terminal_to_net: &FxHashMap<(CompactString, CompactString), CompactString>,
        netlist: &mut PhysicalNetlist,
    ) -> Result<(), DeviceExtractionError> {
        let device_type_id = self.device_registry.get_or_register(device_type);

        let mut terminals = FxHashMap::default();
        for (terminal_name, pour) in terminal_pours {
            let net = if let Some(net_from_route) =
                terminal_to_net.get(&(device_name.into(), terminal_name.clone()))
            {
                net_from_route.to_string()
            } else {
                pour.net.clone().unwrap_or_else(|| "nc".into()).to_string()
            };
            terminals.insert(terminal_name.clone(), net.clone());
            println!("      ├─ {}: {} (net: {})", terminal_name, pour.name, net);
        }

        let mut parameters = FxHashMap::default();

        let is_ic_package = device_type.starts_with('U') || device_type.contains("SOIC") || device_type.contains("QFN");
        
        let mut has_active_region = false;
        for pour in terminal_pours.values() {
            if self.material_database.get_semiconductor(&pour.material_name.to_lowercase()).is_ok() {
                has_active_region = true;
                break;
            }
        }

        if let Some(gate_pour) = terminal_pours.get("gate") {
            if is_ic_package || !has_active_region {
                println!("      ⚠️  Skipping MOSFET extraction for {}: Not a silicon-level transistor", device_name);
            } else {
                let drain_net = terminals.get("drain").map(|s| s.as_str()).unwrap_or("nc");
                let gate_net = terminals.get("gate").map(|s| s.as_str()).unwrap_or("nc");
                let source_net = terminals.get("source").map(|s| s.as_str()).unwrap_or("nc");
                let bulk_net = terminals.get("bulk").map(|s| s.as_str()).unwrap_or("nc");

                if drain_net == gate_net && gate_net == source_net && source_net == bulk_net && drain_net != "nc" {
                    println!("      ⚠️  Skipping MOSFET extraction for {}: Terminals are shorted", device_name);
                    return Ok(());
                }

                let (width_um, length_um) = self.calculate_channel_dimensions(gate_pour)?;
                parameters.insert("W".into(), width_um);
                parameters.insert("L".into(), length_um);
                println!("      ├─ W={:.1}um L={:.1}um", width_um, length_um);

                let source_pour = terminal_pours.get("source");
                let drain_pour = terminal_pours.get("drain");
                if let (Some(s), Some(d)) = (source_pour, drain_pour) {
                    let (as_m2, ad_m2, ps_m, pd_m) = self
                        .calculate_parasitics_from_pours(s, d)
                        .unwrap_or((0.0, 0.0, 0.0, 0.0));
                    parameters.insert("AS".into(), as_m2);
                    parameters.insert("AD".into(), ad_m2);
                    parameters.insert("PS".into(), ps_m);
                    parameters.insert("PD".into(), pd_m);
                    println!(
                        "      └─ Parasitics: AS={:.2e}m² AD={:.2e}m² PS={:.2e}m PD={:.2e}m",
                        as_m2, ad_m2, ps_m, pd_m
                    );
                }

                let bulk_pour = terminal_pours.get("bulk");
                self.validate_bulk_biasing_from_material(
                    bulk_net,
                    device_type,
                    bulk_pour,
                    device_name,
                )?;
            }
        }

        self.validate_device_materials(device_name, device_type, terminal_pours)?;

        let terminal_pours_map: FxHashMap<CompactString, String> = terminal_pours
            .iter()
            .map(|(terminal, pour)| (terminal.clone(), pour.name.to_string()))
            .collect();

        for net_name in terminals.values() {
            netlist
                .nets
                .entry(net_name.clone().into())
                .or_insert_with(|| NetInfo::new(net_name))
                .connected_devices
                .push(device_name.into());
        }

        let device = PhysicalDevice {
            name: device_name.into(),
            device_type_id,
            terminals,
            parameters,
            terminal_pours: terminal_pours_map,
        };

        netlist.devices.push(device);

        Ok(())
    }
}
