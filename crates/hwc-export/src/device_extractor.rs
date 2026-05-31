//! Device Extractor - Phase 4: Silent Atom Architecture
//!
//! This module implements EXPLICIT INTENT-BASED device extraction.
//!
//! # The Silent Atom Philosophy
//!
//! Atoms are **electrically inert** by default. The compiler performs NO guessing:
//! - NO geometric pattern searching
//! - NO adjacency detection  
//! - NO coordinate-based inference
//!
//! Devices are ONLY extracted when the user explicitly binds geometry to logical
//! devices using the `device: DeviceName.terminal` property.
//!
//! # Example
//! ```hardware
//! module Inverter:
//!     add NMOS named M1
//!     route M1.gate to VIN
//!     route M1.source to GND
//!     route M1.drain to VOUT
//!     route M1.bulk to GND
//!
//! space InverterLayout implements Inverter:
//!     add pour(Polysilicon) named Gate on z:3:
//!         device: M1.gate  # ← Explicit binding
//!         net: VIN
//!         boundary: [x: 200um, y: 400um] to [x: 300um, y: 600um]
//!     
//!     add pour(Silicon_N) named Source on z:3:
//!         device: M1.source  # ← Explicit binding
//!         net: GND
//!         boundary: [x: 200um, y: 100um] to [x: 300um, y: 200um]
//!     # ... etc
//! ```

use compact_str::CompactString;
use hwc_compiler::alignment::netlist::{
    DeviceTypeRegistry, NetInfo, PhysicalDevice, PhysicalNetlist,
};
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;
use std::fmt;

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
}

impl Default for ExtractedDevices {
    fn default() -> Self {
        Self::new()
    }
}

/// Device extraction errors
#[derive(Debug, Clone)]
pub enum DeviceExtractionError {
    /// Invalid geometry
    InvalidGeometry {
        device_name: CompactString,
        device_type: CompactString,
        reason: CompactString,
    },
    /// Missing bulk contact (GAP 5)
    MissingBulkContact {
        transistor: CompactString,
        device_type_name: CompactString,
        expected_bulk_net: CompactString,
    },
    /// Bulk biasing violation (GAP 5)
    BiasViolation {
        transistor: CompactString,
        device_type_name: CompactString,
        bulk_net: CompactString,
        expected_net: CompactString,
    },
}

impl fmt::Display for DeviceExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGeometry {
                device_name,
                device_type,
                reason,
            } => {
                write!(
                    f,
                    "Invalid geometry for {} '{}': {}",
                    device_type, device_name, reason
                )
            }
            Self::MissingBulkContact {
                transistor,
                device_type_name,
                expected_bulk_net,
            } => {
                write!(
                    f,
                    "Missing bulk contact for {} '{}': expected connection to {}",
                    device_type_name, transistor, expected_bulk_net
                )
            }
            Self::BiasViolation {
                transistor,
                device_type_name,
                bulk_net,
                expected_net,
            } => {
                write!(
                    f,
                    "Bulk biasing violation for {} '{}': bulk connected to '{}', expected '{}'",
                    device_type_name, transistor, bulk_net, expected_net
                )
            }
        }
    }
}

impl std::error::Error for DeviceExtractionError {}

/// Device extractor using explicit intent-based bindings
pub struct DeviceExtractor<'a> {
    space: &'a HardwareSpace,
    symbol_table: &'a hwc_compiler::SymbolTable,
    device_registry: DeviceTypeRegistry,
    material_database: hwc_materials::MaterialDatabase,
}

impl<'a> DeviceExtractor<'a> {
    /// Create a new device extractor
    pub fn new(space: &'a HardwareSpace, symbol_table: &'a hwc_compiler::SymbolTable) -> Self {
        // Populate material database from symbol table for physics validation
        let material_database = hwc_compiler::populate_material_database(symbol_table)
            .unwrap_or_else(|_| hwc_materials::MaterialDatabase::empty());

        // DEBUG: Print what materials are in the database
        // eprintln!($3"[DEBUG] Material database loaded:");
        // eprintln!($3"[DEBUG]   Conductors: {}", material_database.conductors.len());
        for _name in material_database.conductors.keys() {
            // eprintln!($3"[DEBUG]     - {}", name);
        }
        // eprintln!($3"[DEBUG]   Semiconductors: {}", material_database.semiconductors.len());
        for _name in material_database.semiconductors.keys() {
            // eprintln!($3"[DEBUG]     - {}", name);
        }
        // eprintln!($3"[DEBUG]   Insulators: {}", material_database.insulators.len());
        for _name in material_database.insulators.keys() {
            // eprintln!($3"[DEBUG]     - {}", name);
        }

        Self {
            space,
            symbol_table,
            device_registry: DeviceTypeRegistry::new(),
            material_database,
        }
    }

    /// Extract devices using explicit intent-based bindings (Phase 4: Silent Atom)
    ///
    /// # Arguments
    /// * `module` - Module definition containing logical device declarations (REQUIRED)
    ///
    /// # Returns
    /// PhysicalNetlist with extracted devices, or errors if extraction fails
    ///
    /// # Errors
    /// Returns error if no module is provided - geometric guessing is no longer supported.
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
    ///
    /// The "Checklist" pattern:
    /// 1. For each logical device in the module
    /// 2. Find all pours bound to that device's terminals
    /// 3. Verify all required terminals are bound
    /// 4. Extract the device from the bound geometry
    fn extract_devices_intent_based(
        &mut self,
        module: &hwc_parser::ast::ModuleDefinition,
    ) -> Result<PhysicalNetlist, Vec<DeviceExtractionError>> {
        // Create empty netlist - we'll populate the registry after extraction
        let mut netlist = PhysicalNetlist::new();
        let mut errors = Vec::new();

        // Step 1: Group all pours by their device binding
        let bindings = self.group_pours_by_device_binding();

        // Step 2: Extract devices and their terminals from module statements
        let extracted = self.extract_devices_from_module(module);

        // Step 2.5: Build terminal-to-net mapping from module route statements
        // This is the SOURCE OF TRUTH for net assignments, not the component definitions
        let (terminal_to_net, all_net_names) = self.build_terminal_to_net_mapping(module);

        // eprintln!($3"[DEBUG] Extracted {} devices from module", extracted.devices.len());
        for (_name, _dtype) in &extracted.devices {
            // eprintln!($3"[DEBUG]   Device: {} -> Type: {}", name, dtype);
        }

        // Step 3: For each logical device, verify all terminals are bound
        for (device_name, device_type) in extracted.devices {
            // eprintln!($3"[DEBUG] Checking device: {} (type: {})", device_name, device_type);
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
            // Add all net names (including aliases) to the physical netlist
            // This ensures the net count matches the logical netlist
            // eprintln!($3"[DEBUG] Adding {} net names to physical netlist:", all_net_names.len());
            for _net_name in &all_net_names {
                // eprintln!($3"[DEBUG]   Net: {}", net_name);
            }

            for net_name in all_net_names {
                netlist
                    .nets
                    .entry(net_name.clone())
                    .or_insert_with(|| NetInfo::new(&net_name));
            }

            // Copy port information from module (explicit intent, no inference!)
            self.copy_ports_from_module(module, &mut netlist);

            // Update the netlist's device registry with all registered types
            netlist.device_registry = self.device_registry.clone();

            Ok(netlist)
        } else {
            Err(errors)
        }
    }

    /// Copy port declarations from module to physical netlist
    ///
    /// This is the CORRECT approach: we don't infer or guess port directions.
    /// We simply copy the explicit declarations from the module.
    ///
    /// Build terminal-to-net mapping from module route statements
    ///
    /// This extracts the SOURCE OF TRUTH for net assignments from the module's
    /// route statements, which override any hardcoded net names in component definitions.
    ///
    /// This function builds a net equivalence graph where all connected terminals
    /// share the same net name. For example:
    /// ```
    /// route M1.gate to STAGE1_IN
    /// route M5.drain to M1.gate
    /// ```
    /// Creates a single net "STAGE1_IN" that includes both M1.gate and M5.drain.
    ///
    /// Returns: (terminal_to_net_mapping, all_net_names)
    fn build_terminal_to_net_mapping(
        &self,
        module: &hwc_parser::ast::ModuleDefinition,
    ) -> (
        FxHashMap<(CompactString, CompactString), CompactString>,
        std::collections::HashSet<CompactString>,
    ) {
        use hwc_parser::ast::ModuleStatement;

        // Use union-find to properly merge nets across device-to-device connections
        let mut terminal_to_net: FxHashMap<String, String> = FxHashMap::default();
        let mut net_parent: FxHashMap<String, String> = FxHashMap::default();
        let mut all_net_names = std::collections::HashSet::new();

        // Helper function to find root net (with path compression)
        fn find_root(net: &str, net_parent: &mut FxHashMap<String, String>) -> String {
            if let Some(parent) = net_parent.get(net).cloned() {
                if parent != net {
                    let root = find_root(&parent, net_parent);
                    net_parent.insert(net.to_string(), root.clone());
                    return root;
                }
            }
            net.to_string()
        }

        // Helper function to union two nets
        fn union_nets(net1: &str, net2: &str, net_parent: &mut FxHashMap<String, String>) {
            let root1 = find_root(net1, net_parent);
            let root2 = find_root(net2, net_parent);
            if root1 != root2 {
                // Prefer explicit net names over implicit ones
                // Explicit names don't contain "." or "__"
                let is_explicit1 = !root1.contains('.') && !root1.contains("__");
                let is_explicit2 = !root2.contains('.') && !root2.contains("__");

                if is_explicit1 && !is_explicit2 {
                    net_parent.insert(root2, root1);
                } else if is_explicit2 && !is_explicit1 {
                    net_parent.insert(root1, root2);
                } else {
                    // Both explicit or both implicit - use lexicographic order
                    if root1 < root2 {
                        net_parent.insert(root2, root1);
                    } else {
                        net_parent.insert(root1, root2);
                    }
                }
            }
        }

        // Process route statements to build connectivity
        for statement in &module.statements {
            if let ModuleStatement::Route(route) = statement {
                let from_component = &route.from.component;
                let from_pin = &route.from.pin;
                let to_component = &route.to.component;
                let to_pin = &route.to.pin;

                // Build terminal identifiers
                let from_is_device = !from_component.is_empty();
                let to_is_device = !to_component.is_empty();

                let from_terminal = if from_is_device {
                    format!("{}.{}", from_component, from_pin)
                } else {
                    from_pin.to_string()
                };

                let to_terminal = if to_is_device {
                    format!("{}.{}", to_component, to_pin)
                } else {
                    to_pin.to_string()
                };

                // Note: We don't add net names here - we'll add canonical names after union-find

                // Get or create net names for both sides
                let from_net = terminal_to_net
                    .entry(from_terminal.clone())
                    .or_insert_with(|| from_terminal.clone())
                    .clone();
                let to_net = terminal_to_net
                    .entry(to_terminal.clone())
                    .or_insert_with(|| to_terminal.clone())
                    .clone();

                // Initialize in union-find if needed
                net_parent
                    .entry(from_net.clone())
                    .or_insert_with(|| from_net.clone());
                net_parent
                    .entry(to_net.clone())
                    .or_insert_with(|| to_net.clone());

                // Union the two nets
                union_nets(&from_net, &to_net, &mut net_parent);
            }
        }

        // Resolve all terminals to their canonical net names
        let mut result: FxHashMap<(CompactString, CompactString), CompactString> =
            FxHashMap::default();
        for terminal in terminal_to_net.keys() {
            if let Some((device_name, pin_name)) = terminal.split_once('.') {
                let net_name = find_root(terminal, &mut net_parent);
                result.insert(
                    (device_name.into(), pin_name.into()),
                    net_name.clone().into(),
                );

                // Track only the canonical net name (after merging)
                all_net_names.insert(net_name.into());
            }
        }

        (result, all_net_names)
    }

    /// Copy port declarations from module to physical netlist
    ///
    /// This is the CORRECT approach: we don't infer or guess port directions.
    /// We simply copy the explicit declarations from the module.
    ///
    /// For each net in the physical layout:
    /// 1. Look it up in the module's pin declarations
    /// 2. Copy the direction from the module
    /// 3. If not found in module, it's an internal net (not a port)
    fn copy_ports_from_module(
        &self,
        module: &hwc_parser::ast::ModuleDefinition,
        netlist: &mut PhysicalNetlist,
    ) {
        use hwc_compiler::alignment::netlist::{PortDirection, PortInfo};

        // Build a map of module pins: name -> direction
        let mut module_pins: rustc_hash::FxHashMap<CompactString, hwc_parser::PinDirection> =
            rustc_hash::FxHashMap::default();
        for pin in &module.pins {
            module_pins.insert(pin.name.clone(), pin.direction);
        }

        // Collect all unique net names from devices
        let mut net_names = rustc_hash::FxHashSet::default();
        for device in &netlist.devices {
            for net_name in device.terminals.values() {
                net_names.insert(net_name.clone());
            }
        }

        // For each net, check if it's declared as a port in the module
        for net_name in net_names {
            if let Some(module_direction) = module_pins.get(net_name.as_str()) {
                // This net is a port - copy the direction from the module
                let direction = match module_direction {
                    hwc_parser::PinDirection::Input => PortDirection::Input,
                    hwc_parser::PinDirection::Output => PortDirection::Output,
                    hwc_parser::PinDirection::Inout => PortDirection::Inout,
                    hwc_parser::PinDirection::Power => PortDirection::Power,
                    hwc_parser::PinDirection::Ground => PortDirection::Ground,
                    hwc_parser::PinDirection::Passive => PortDirection::Inout,
                };

                netlist.ports.push(PortInfo {
                    name: net_name.into(),
                    direction,
                });
            }
            // If not in module pins, it's an internal net (not a port) - skip it
        }

        println!(
            "   ├─ Copied {} port declarations from module",
            netlist.ports.len()
        );
    }
    /// Since spaces don't have explicit port syntax yet, we extract port names
    /// Group all pours by their device binding
    ///
    /// Creates a map: DeviceName -> (Terminal -> PourMetadata)
    fn group_pours_by_device_binding(
        &self,
    ) -> FxHashMap<CompactString, FxHashMap<CompactString, hwc_engine::space::PourMetadata>> {
        let mut bindings: FxHashMap<
            CompactString,
            FxHashMap<CompactString, hwc_engine::space::PourMetadata>,
        > = FxHashMap::default();

        println!(
            "   ├─ Scanning {} pours for device bindings...",
            self.space.pours.len()
        );

        for pour in &self.space.pours {
            println!(
                "      ├─ Pour '{}': device_binding = {:?}",
                pour.name, pour.device_binding
            );

            if let Some(ref device_binding) = pour.device_binding {
                let device_name = &device_binding.device_name;
                let terminal = &device_binding.terminal;

                bindings
                    .entry(device_name.clone())
                    .or_default()
                    .insert(terminal.clone(), pour.clone());

                println!(
                    "      ├─ Bound: {}.{} → {} ({})",
                    device_name, terminal, pour.name, pour.material_name
                );
            }
        }

        bindings
    }

    /// Extract devices and their terminals from module statements
    ///
    /// Parses the module to find:
    /// 1. Which devices exist (from `add` statements)
    /// 2. Which terminals each device uses (from `route` statements)
    fn extract_devices_from_module(
        &self,
        module: &hwc_parser::ast::ModuleDefinition,
    ) -> ExtractedDevices {
        use hwc_parser::ast::ModuleStatement;

        let mut extracted = ExtractedDevices::new();

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

    /// Extract a device from explicitly bound geometry
    fn extract_bound_device(
        &mut self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, hwc_engine::space::PourMetadata>,
        terminal_to_net: &FxHashMap<(CompactString, CompactString), CompactString>, // (device, terminal) -> net
        netlist: &mut PhysicalNetlist,
    ) -> Result<(), DeviceExtractionError> {
        // eprintln!($3"[DEBUG extract_bound_device] Device: {}, Type: {}", device_name, device_type);
        // eprintln!($3"[DEBUG extract_bound_device] Terminal pours: {:?}", terminal_pours.keys().collect::<Vec<_>>());

        let device_type_id = self.device_registry.get_or_register(device_type);

        // Extract terminal connections (GENERIC - works for ANY device type)
        // CRITICAL FIX: Use module route statements as source of truth, not pour metadata
        let mut terminals = FxHashMap::default();
        for (terminal_name, pour) in terminal_pours {
            // Look up net from module route statements first
            let net = if let Some(net_from_route) =
                terminal_to_net.get(&(device_name.into(), terminal_name.clone()))
            {
                net_from_route.to_string()
            } else {
                // Fallback to pour metadata (for standalone components not in modules)
                pour.net.clone().unwrap_or_else(|| "nc".into()).to_string()
            };
            terminals.insert(terminal_name.clone(), net.clone());
            println!("      ├─ {}: {} (net: {})", terminal_name, pour.name, net);
        }

        // Build parameter map (GENERIC - extract from geometry, not hardcoded)
        let mut parameters = FxHashMap::default();

        // For transistors: calculate W/L from gate geometry
        if let Some(gate_pour) = terminal_pours.get("gate") {
            // eprintln!($3"[DEBUG extract_bound_device] Transistor detected (has 'gate' terminal), calculating W/L");
            let (width_um, length_um) = self.calculate_channel_dimensions(gate_pour)?;
            parameters.insert("W".into(), width_um);
            parameters.insert("L".into(), length_um);
            println!("      ├─ W={:.1}um L={:.1}um", width_um, length_um);

            // Calculate parasitic parameters for transistors
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

            // Validate bulk biasing for transistors (GAP 5)
            let bulk_net = terminals.get("bulk").map(|s| s.as_str()).unwrap_or("nc");
            let bulk_pour = terminal_pours.get("bulk");
            self.validate_bulk_biasing_from_material(
                bulk_net,
                device_type,
                bulk_pour,
                device_name,
            )?;
        } else {
            // eprintln!($3"[DEBUG extract_bound_device] Non-transistor device (no 'gate' terminal), skipping W/L calculation");
            // For non-transistor devices (resistors, capacitors, etc.), no special parameters needed
            // The terminals and nets are sufficient
        }

        // Validate materials against device definition (GAP 7: Material Validation)
        // This works for ALL device types
        self.validate_device_materials(device_name, device_type, terminal_pours)?;

        // Build terminal_pours map for spatial error reporting
        let terminal_pours_map: FxHashMap<CompactString, String> = terminal_pours
            .iter()
            .map(|(terminal, pour)| (terminal.clone(), pour.name.to_string()))
            .collect();

        // Update net info BEFORE moving terminals into the device
        // CRITICAL FIX: Use the resolved terminal nets (from module routes), not pour metadata
        for net_name in terminals.values() {
            netlist
                .nets
                .entry(net_name.clone().into())
                .or_insert_with(|| NetInfo::new(net_name))
                .connected_devices
                .push(device_name.into());
        }

        // Create physical device
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

    /// Calculate parasitic parameters from source/drain pours
    fn calculate_parasitics_from_pours(
        &self,
        source_pour: &hwc_engine::space::PourMetadata,
        drain_pour: &hwc_engine::space::PourMetadata,
    ) -> Option<(f64, f64, f64, f64)> {
        let as_m2 = (source_pour.area_nm2 as f64) / 1e18;
        let ad_m2 = (drain_pour.area_nm2 as f64) / 1e18;

        let ps_m = source_pour
            .bbox
            .as_ref()
            .map(|bbox| self.calculate_perimeter(bbox))
            .unwrap_or(0.0);

        let pd_m = drain_pour
            .bbox
            .as_ref()
            .map(|bbox| self.calculate_perimeter(bbox))
            .unwrap_or(0.0);

        Some((as_m2, ad_m2, ps_m, pd_m))
    }

    /// Calculate perimeter of a bounding box
    fn calculate_perimeter(&self, bbox: &hwc_engine::geometry::BoundingBox) -> f64 {
        let width_nm = (bbox.max.x - bbox.min.x).abs() as f64;
        let height_nm = (bbox.max.y - bbox.min.y).abs() as f64;
        let perimeter_nm = 2.0 * (width_nm + height_nm);
        perimeter_nm / 1e9
    }

    /// Calculate channel dimensions from gate geometry
    fn calculate_channel_dimensions(
        &self,
        gate_pour: &hwc_engine::space::PourMetadata,
    ) -> Result<(f64, f64), DeviceExtractionError> {
        let area_nm2 = gate_pour.area_nm2 as f64;
        let side_nm = area_nm2.sqrt();
        let side_um = side_nm / 1000.0;
        Ok((side_um, side_um))
    }

    /// Validate bulk biasing based on material properties (GAP 5)
    ///
    /// ARCHITECTURE: Physics-driven validation using material database.
    ///
    /// This function reads doping_type and bias_requirement from the material
    /// database instead of hardcoding material names or device types.
    ///
    /// Flow:
    /// 1. Get bulk material name from pour
    /// 2. Look up material in database
    /// 3. Read bias_requirement property
    /// 4. Get net classification from space
    /// 5. Validate: does net classification match material requirement?
    ///
    /// This scales infinitely - works for ANY semiconductor material!
    fn validate_bulk_biasing_from_material(
        &self,
        bulk_net: &str,
        device_type_name: &str,
        bulk_pour: Option<&hwc_engine::space::PourMetadata>,
        transistor_name: &str,
    ) -> Result<(), DeviceExtractionError> {
        // Get the bulk material from the pour
        let bulk_material = match bulk_pour {
            Some(pour) => &pour.material_name,
            None => {
                // No bulk pour - skip validation (will be caught by missing terminal check)
                return Ok(());
            }
        };

        // Look up material in database (case-insensitive)
        let semiconductor = match self
            .material_database
            .get_semiconductor(&bulk_material.to_lowercase())
        {
            Ok(semi) => semi,
            Err(_) => {
                // Material not in database - skip validation
                // This allows custom materials without breaking the compiler
                println!(
                    "   ⚠️  Warning: Material '{}' not found in database, skipping bias validation",
                    bulk_material
                );
                return Ok(());
            }
        };

        // Get bias requirement from material properties
        let bias_req = match &semiconductor.bias_requirement {
            Some(req) => req,
            None => {
                // No bias requirement - material doesn't need biasing (e.g., intrinsic)
                return Ok(());
            }
        };

        // Get net classification from space
        let net_classification = self.space.get_net_classification(bulk_net);

        // Check if net is classified
        if matches!(
            net_classification,
            hwc_engine::space::NetClassification::Unclassified
        ) {
            return Err(DeviceExtractionError::BiasViolation {
                transistor: transistor_name.to_string().into(),
                device_type_name: device_type_name.to_string().into(),
                bulk_net: bulk_net.to_string().into(),
                expected_net: format!(
                    "{:?} classification required by material {} (net '{}' is unclassified - add net_classifications to space)",
                    bias_req, bulk_material, bulk_net
                ).into(),
            });
        }

        // Convert hwc_engine::NetClassification to hwc_materials::NetClassification
        let materials_net_class = match net_classification {
            hwc_engine::space::NetClassification::Power => hwc_materials::NetClassification::Power,
            hwc_engine::space::NetClassification::Ground => {
                hwc_materials::NetClassification::Ground
            }
            hwc_engine::space::NetClassification::Signal => {
                hwc_materials::NetClassification::Signal
            }
            hwc_engine::space::NetClassification::HighVoltage => {
                hwc_materials::NetClassification::HighVoltage
            }
            hwc_engine::space::NetClassification::Unclassified => {
                hwc_materials::NetClassification::Unclassified
            }
        };

        // Validate using the material's bias requirement method (data-driven!)
        if let Err(reason) = bias_req.validate_net_classification(materials_net_class) {
            return Err(DeviceExtractionError::BiasViolation {
                transistor: transistor_name.to_string().into(),
                device_type_name: format!(
                    "{} ({} bulk: {})",
                    device_type_name,
                    semiconductor
                        .doping_type
                        .as_ref()
                        .map(|dt| format!("{:?}", dt))
                        .unwrap_or_else(|| "unknown".to_string()),
                    bulk_material
                )
                .into(),
                bulk_net: bulk_net.to_string().into(),
                expected_net: reason.into(),
            });
        }

        Ok(())
    }

    /// Validate device materials against device definition (GAP 7: Material Validation)
    ///
    /// Checks that the materials used for each terminal match the expected materials
    /// defined in the device definition from the standard library.
    ///
    /// Sprint 1.5 Enhancement: Uses device contracts for advanced validation
    fn validate_device_materials(
        &self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, hwc_engine::space::PourMetadata>,
    ) -> Result<(), DeviceExtractionError> {
        // Try to get device definition from symbol table
        let device_def = match self.symbol_table.get_device(device_type) {
            Ok(def) => def,
            Err(_) => {
                // Device definition not found - this is OK, validation is optional
                // User may be using custom device types not in stdlib
                return Ok(());
            }
        };

        // Convert device definition to contract for validation
        let contract = hwc_parser::DeviceContract::from_device_definition(device_def);

        // Validate each terminal's material using contract
        for (terminal_name, pour) in terminal_pours {
            if let Err(reason) =
                contract.validate_terminal_material(terminal_name, &pour.material_name)
            {
                return Err(DeviceExtractionError::InvalidGeometry {
                    device_name: device_name.to_string().into(),
                    device_type: device_type.to_string().into(),
                    reason: format!(
                        "❌ Physics Error: {} device contract violation\n\n  \
                        Device: {} ({})\n  \
                        Terminal: {}\n  \
                        Pour: {}\n  \
                        {}\n  \
                        Contract: @std/foundry/transistors.hw::{}",
                        device_type,
                        device_name,
                        device_type,
                        terminal_name,
                        pour.name,
                        reason,
                        device_type
                    )
                    .into(),
                });
            }
        }

        Ok(())
    }
}

/// Format physical netlist as SPICE text
///
/// Converts the structured PhysicalNetlist into SPICE netlist format
pub fn format_spice(
    netlist: &PhysicalNetlist,
    device_registry: &DeviceTypeRegistry,
) -> CompactString {
    let mut spice = String::new();

    for device in &netlist.devices {
        let device_type = device_registry
            .get_name(device.device_type_id)
            .unwrap_or("UNKNOWN");

        // Get terminal connections with a default value that lives long enough
        let nc = "nc".into();
        let drain = device.terminals.get("drain").unwrap_or(&nc);
        let gate = device.terminals.get("gate").unwrap_or(&nc);
        let source = device.terminals.get("source").unwrap_or(&nc);
        let bulk = device.terminals.get("bulk").unwrap_or(&nc);

        // Get parameters
        let w = device.parameters.get("W").copied().unwrap_or(0.0);
        let l = device.parameters.get("L").copied().unwrap_or(0.0);

        // Format: M<name> <drain> <gate> <source> <bulk> <type> W=<w>u L=<l>u
        spice.push_str(&format!(
            "M{} {} {} {} {} {} W={}u L={}u",
            device.name, drain, gate, source, bulk, device_type, w, l
        ));

        // Add parasitic parameters if available
        if let Some(&as_val) = device.parameters.get("AS") {
            spice.push_str(&format!(" AS={:.2e}", as_val));
        }
        if let Some(&ad_val) = device.parameters.get("AD") {
            spice.push_str(&format!(" AD={:.2e}", ad_val));
        }
        if let Some(&ps_val) = device.parameters.get("PS") {
            spice.push_str(&format!(" PS={:.2e}", ps_val));
        }
        if let Some(&pd_val) = device.parameters.get("PD") {
            spice.push_str(&format!(" PD={:.2e}", pd_val));
        }

        spice.push('\n');
    }

    spice.into()
}
