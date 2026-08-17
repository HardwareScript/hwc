//! Device Extractor - Phase 4: Silent Atom Architecture
//!
//! This module implements EXPLICIT INTENT-BASED device extraction.

pub mod continuity;
pub mod error;
pub mod extracted;
pub mod geometry;
pub mod mapping;
pub mod parameter_extraction;
pub mod spice;
pub mod validation;

pub use continuity::{
    ChannelContinuityReport, ChannelElement, ChannelIsland, ChannelNodeId, DeviceChannelGraph,
    DeviceTopologyValidator,
};
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
    pub(super) arena: &'a hwc_parser::ast::AstArena,
    pub(super) space_def: Option<&'a hwc_parser::SpaceDefinition>, // v0.2.1: For device_nets access
    pub(super) device_registry: DeviceTypeRegistry,
}

impl<'a> DeviceExtractor<'a> {
    pub fn new(
        space: &'a HardwareSpace,
        symbol_table: &'a hwc_compiler::SymbolTable,
        arena: &'a hwc_parser::ast::AstArena,
        space_def: Option<&'a hwc_parser::SpaceDefinition>,
    ) -> Self {
        Self {
            space,
            symbol_table,
            arena,
            space_def,
            device_registry: DeviceTypeRegistry::new(),
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

        // Step 2: Extract devices from BOTH module statements AND pour bindings
        let mut extracted = ExtractedDevices::from_module(module, self.arena);
        let from_bindings = ExtractedDevices::from_pour_bindings(&bindings, self.symbol_table);

        // Merge devices from bindings into extracted
        for (device_name, device_type) in from_bindings.devices {
            extracted.devices.push((device_name.clone(), device_type));
        }
        for (device_name, terminals) in from_bindings.device_terminals {
            extracted
                .device_terminals
                .entry(device_name)
                .or_default()
                .extend(terminals);
        }

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

    /// Helper: Find a net from overlapping contacts/vias when pour itself is 'nc'
    fn find_overlapping_net(&self, pour: &hwc_engine::space::PourMetadata) -> Option<String> {
        let pour_bbox = pour.bbox.as_ref()?;

        // Check all contacts for overlaps
        for contact in &self.space.contacts {
            if let Some(contact_bbox) = &contact.bbox {
                if Self::bboxes_overlap(pour_bbox, contact_bbox) {
                    if let Some(net) = &contact.net {
                        if net.as_str() != "nc" && !net.is_empty() {
                            return Some(net.to_string());
                        }
                    }
                }
            }
        }

        // Check all pours for overlaps (for vias or other connecting pours)
        for other_pour in &self.space.pours {
            if other_pour.name == pour.name {
                continue; // Skip self
            }

            if let Some(other_bbox) = &other_pour.bbox {
                if Self::bboxes_overlap(pour_bbox, other_bbox) {
                    if let Some(net) = &other_pour.net {
                        if net.as_str() != "nc" && !net.is_empty() {
                            return Some(net.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    /// Helper: Check if two bounding boxes overlap
    fn bboxes_overlap(
        a: &hwc_engine::geometry::BoundingBox,
        b: &hwc_engine::geometry::BoundingBox,
    ) -> bool {
        // Bounding boxes overlap if they intersect in all 3 dimensions
        let x_overlap = a.min.x < b.max.x && a.max.x > b.min.x;
        let y_overlap = a.min.y < b.max.y && a.max.y > b.min.y;
        let z_overlap = a.min.z < b.max.z && a.max.z > b.min.z;

        x_overlap && y_overlap && z_overlap
    }

    /// Helper: Get device terminal net from space-level device_nets declaration
    /// 
    /// Returns the net name if an explicit device_nets binding exists
    fn get_device_net_from_space(&self, device_name: &str, terminal_name: &CompactString) -> Option<String> {
        eprintln!("[DEVICE_NETS DEBUG] Looking up device='{}', terminal='{}'", device_name, terminal_name);
        eprintln!("[DEVICE_NETS DEBUG] space_def.is_some(): {}", self.space_def.is_some());
        
        let result = self.space_def
            .and_then(|space_def| {
                eprintln!("[DEVICE_NETS DEBUG] space_def.device_nets has {} entries", space_def.device_nets.len());
                eprintln!("[DEVICE_NETS DEBUG] space_def.device_nets keys: {:?}", space_def.device_nets.keys().collect::<Vec<_>>());
                
                space_def.device_nets
                    .get(device_name)
                    .and_then(|terminal_map| {
                        eprintln!("[DEVICE_NETS DEBUG] Found device '{}' in device_nets with {} terminals", device_name, terminal_map.len());
                        eprintln!("[DEVICE_NETS DEBUG] Terminal map keys: {:?}", terminal_map.keys().collect::<Vec<_>>());
                        terminal_map.get(terminal_name)
                    })
                    .map(|net| net.to_string())
            });
        
        eprintln!("[DEVICE_NETS DEBUG] Result: {:?}", result);
        result
    }

    /// Extract a device from explicitly bound geometry
    fn extract_bound_device(
        &mut self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, Vec<hwc_engine::space::PourMetadata>>,
        terminal_to_net: &FxHashMap<(CompactString, CompactString), CompactString>,
        netlist: &mut PhysicalNetlist,
    ) -> Result<(), DeviceExtractionError> {
        eprintln!("[EXTRACT_DEVICE] Starting extraction for device='{}', type='{}'", device_name, device_type);
        eprintln!("[EXTRACT_DEVICE] terminal_pours.len()={}", terminal_pours.len());
        eprintln!("[EXTRACT_DEVICE] terminal_pours.keys()={:?}", terminal_pours.keys().collect::<Vec<_>>());
        
        let device_type_id = self.device_registry.get_or_register(device_type);

        let mut terminals = FxHashMap::default();
        
        // Step 1: Map physical terminals (those with geometry) to their nets
        for (terminal_name, pours) in terminal_pours {
            // FAIL LOUDLY: Empty pour vector should never happen
            if pours.is_empty() {
                return Err(DeviceExtractionError::InvalidGeometry {
                    device_name: device_name.to_string().into(),
                    device_type: device_type.to_string().into(),
                    reason: format!(
                        "Terminal '{}' has empty pour vector (internal compiler error). \
                         This should never happen.",
                        terminal_name
                    )
                    .into(),
                });
            }

            // Determine net from route declaration or pour bindings
            let net = if let Some(net_from_route) =
                terminal_to_net.get(&(device_name.into(), terminal_name.clone()))
            {
                net_from_route.to_string()
            } else {
                // Check all pours for this terminal to find the first with a valid net
                // v0.2.2: Sort by priority so Contact pours (high priority) are checked first
                let mut sorted_pours = pours.clone();
                sorted_pours.sort_by_key(|p| p.device_binding.as_ref().map_or(
                    hwc_engine::space::BindingPriority::default(),
                    |b| b.priority
                ));
                sorted_pours.reverse(); // High priority (Contact=100) first

                let mut found_net: Option<String> = None;
                for pour in &sorted_pours {
                    let pour_net = pour.net.clone().unwrap_or_else(|| "nc".into());
                    if pour_net.as_str() != "nc" && !pour_net.is_empty() {
                        found_net = Some(pour_net.to_string());
                        break;
                    }
                }

                if let Some(net) = found_net {
                    net
                } else {
                    // All pours are 'nc' - check for overlapping contacts
                    // FAIL LOUDLY: If no net found anywhere, error out
                    let first_pour = pours.first().ok_or_else(|| {
                        DeviceExtractionError::InvalidGeometry {
                            device_name: device_name.to_string().into(),
                            device_type: device_type.to_string().into(),
                            reason: format!(
                                "Terminal '{}' pour vector is not empty but .first() returned None (internal error)",
                                terminal_name
                            )
                            .into(),
                        }
                    })?;

                    self.find_overlapping_net(first_pour).ok_or_else(|| {
                        DeviceExtractionError::InvalidGeometry {
                            device_name: device_name.to_string().into(),
                            device_type: device_type.to_string().into(),
                            reason: format!(
                                "Terminal '{}' has no net assignment. \
                                 All {} pours bound to this terminal are marked 'nc' and no overlapping \
                                 contacts/vias found to inherit net from.\n\
                                 \n\
                                 Fix: Add 'net: <NetName>' to one of the pours bound to {}.{}",
                                terminal_name,
                                pours.len(),
                                device_name,
                                terminal_name
                            )
                            .into(),
                        }
                    })?
                }
            };

            terminals.insert(terminal_name.clone(), net.clone());

            eprintln!("[EXTRACT_DEVICE] Inserted physical terminal '{}' -> '{}' into terminals map", terminal_name, net);

            // Print all pours bound to this terminal
            for pour in pours {
                println!(
                    "      ├─ {} (physical): {} ({}) → net '{}'",
                    terminal_name, pour.name, pour.material_name, net
                );
            }
        }

        // Step 2: Map virtual terminals (those WITHOUT geometry) to their nets from device contract
        // This supports non-physical terminals like BULK, SUBSTRATE that only need net connections
        eprintln!("[EXTRACT_DEVICE] Step 2: Checking for virtual terminals");
        eprintln!("[EXTRACT_DEVICE] Attempting symbol_table.get_device('{}')", device_type);
        
        if let Ok(device_def) = self.symbol_table.get_device(device_type) {
            eprintln!("[EXTRACT_DEVICE] Found device_def with {} terminals", device_def.terminals.len());
            eprintln!("[EXTRACT_DEVICE] device_def.terminals={:?}", device_def.terminals);
            eprintln!("[EXTRACT_DEVICE] device_def.materials keys={:?}", device_def.materials.keys().collect::<Vec<_>>());

            for terminal_name in &device_def.terminals {
                // Skip if already mapped (physical terminal)
                if terminals.contains_key(terminal_name) {
                    continue;
                }

                // Check if this is a virtual terminal by looking at its material constraints
                // Virtual terminals use Air or Vacuum in the device contract
                let is_virtual = if let Some(allowed_materials) = device_def.materials.get(terminal_name) {
                    allowed_materials.iter().any(|mat_name| {
                        mat_name.as_str() == "Air" || mat_name.as_str() == "Vacuum"
                    })
                } else {
                    false
                };

                if is_virtual {
                    // For virtual terminals, look up net from terminal_to_net map OR device_nets
                    // Priority: device_nets (explicit space-level binding) > terminal_to_net (route statements)
                    let net = if let Some(net_from_device_nets) = self.get_device_net_from_space(device_name, terminal_name) {
                        // Found explicit device_nets binding in space
                        net_from_device_nets
                    } else if let Some(net_from_route) = terminal_to_net.get(&(device_name.into(), terminal_name.clone())) {
                        // Found route-based binding (legacy)
                        net_from_route.to_string()
                    } else {
                        // NO FALLBACK - error loudly if unbound
                        return Err(DeviceExtractionError::InvalidGeometry {
                            device_name: device_name.to_string().into(),
                            device_type: device_type.to_string().into(),
                            reason: format!(
                                "Virtual terminal '{}' has no net mapping.\n\
                                 \n\
                                 Virtual terminals (material: Air) don't require physical geometry,\n\
                                 but they MUST have an explicit net assignment.\n\
                                 \n\
                                 Fix: Add a device_nets declaration in your space:\n\
                                 \n\
                                 space {}:\n\
                                   nets:\n\
                                     GND: {{ classification: ground }}\n\
                                   \n\
                                   device_nets {}:\n\
                                     {}: GND\n\
                                 \n\
                                 Device contract specifies:\n\
                                   device {}:\n\
                                     terminals: {:?}\n\
                                     materials:\n\
                                       {}: Air  # ← Virtual terminal",
                                terminal_name,
                                device_name,
                                device_name,
                                terminal_name,
                                device_type,
                                device_def.terminals,
                                terminal_name
                            )
                            .into(),
                        });
                    };
                    
                    terminals.insert(terminal_name.clone(), net.clone());
                    eprintln!("[EXTRACT_DEVICE] Inserted virtual terminal '{}' -> '{}' into terminals map", terminal_name, net);
                    println!("      ├─ {} (virtual): → net '{}'", terminal_name, net);
                }
            }
        }

        // ✅ UNIVERSAL PARAMETER EXTRACTION: Driven by device contract, not device type
        // Extract parameters using the universal parameter-driven system
        // FAIL LOUDLY: Parameter extraction errors are NOT swallowed
        let parameters = parameter_extraction::extract_parameters_universal(
            device_type,
            terminal_pours,
            self.space,
            self.symbol_table,
        )
        .map_err(|err| {
            DeviceExtractionError::InvalidGeometry {
                device_name: device_name.to_string().into(),
                device_type: device_type.to_string().into(),
                reason: err.into(),
            }
        })?;

        // ✅ ZERO-MAGIC ARCHITECTURE: All parameter extraction now delegated to
        // parameter_extraction::extract_parameters_universal() which uses ONLY
        // spice.prefix to dispatch to the correct extractor.
        //
        // NO MORE:
        // - is_mosfet_device heuristic (checking for W/L parameters)
        // - is_ic_package string matching (starts_with('U'), contains("SOIC"))
        // - gate terminal insulator detection hack
        // - dual-authority parameter extraction
        //
        // The spice.prefix is the single source of truth.

        self.validate_device_materials(device_name, device_type, terminal_pours)?;
        self.validate_device_channel_continuity(device_name, device_type, terminal_pours)?;

        let terminal_pours_map: FxHashMap<CompactString, String> = terminal_pours
            .iter()
            .map(|(terminal, pours)| {
                let pour_names: Vec<String> = pours.iter().map(|p| p.name.to_string()).collect();
                (terminal.clone(), pour_names.join(", "))
            })
            .collect();

        for net_name in terminals.values() {
            netlist
                .nets
                .entry(net_name.clone().into())
                .or_insert_with(|| NetInfo::new(net_name))
                .connected_devices
                .push(device_name.into());
        }

        eprintln!("[EXTRACT_DEVICE] About to create PhysicalDevice '{}' with terminals: {:?}", device_name, terminals);

        let device = PhysicalDevice {
            name: device_name.into(),
            device_type_id,
            terminals,
            parameters,
            terminal_pours: terminal_pours_map,
        };

        eprintln!("[EXTRACT_DEVICE] Created PhysicalDevice '{}', terminals={:?}", device.name, device.terminals);

        netlist.devices.push(device);

        Ok(())
    }
}
