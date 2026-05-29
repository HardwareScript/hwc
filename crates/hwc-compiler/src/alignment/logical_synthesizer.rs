//! Logical Netlist Synthesizer
//!
//! This module converts a `module` definition into a logical netlist that can be
//! compared against the physical netlist extracted from geometry.
//!
//! # Design Philosophy
//!
//! The logical synthesizer walks the already-parsed module AST to extract:
//! - Port declarations (from pins field)
//! - Device instantiations (from AddComponent statements)
//! - Net connections (from Route statements)
//!
//! # Data-Driven Architecture
//!
//! Device types are NOT hardcoded. The synthesizer dynamically registers device
//! types from the module definition, just like MaterialRegistry registers materials.
//!
//! # Example Module (Hardware Script v0.1.6 syntax)
//!
//! ```hardware
//! module Inverter_Logic:
//!     pins: [
//!         input VIN,
//!         output VOUT,
//!         power VDD,
//!         ground GND
//!     ]
//!     
//!     add NMOS named M1
//!     route M1.drain to VOUT
//!     route M1.gate to VIN
//!     route M1.source to GND
//!     route M1.bulk to GND
//!     
//!     add PMOS named M2
//!     route M2.drain to VOUT
//!     route M2.gate to VIN
//!     route M2.source to VDD
//!     route M2.bulk to VDD
//! ```

use super::error::AlignmentError;
use super::netlist::{
    DeviceTypeRegistry, LogicalDevice, LogicalNetlist, NetInfo, PortDirection, PortInfo,
};
use compact_str::CompactString;
use hwc_parser::{ModuleDefinition, ModuleStatement};
use rustc_hash::FxHashMap;

/// Logical netlist synthesizer
pub struct LogicalSynthesizer {
    /// Device type registry for dynamic device type registration
    device_registry: DeviceTypeRegistry,
}

impl LogicalSynthesizer {
    /// Create a new logical synthesizer
    pub fn new() -> Self {
        Self {
            device_registry: DeviceTypeRegistry::new(),
        }
    }

    /// Get reference to device type registry
    pub fn device_registry(&self) -> &DeviceTypeRegistry {
        &self.device_registry
    }

    /// Get mutable reference to device type registry
    pub fn device_registry_mut(&mut self) -> &mut DeviceTypeRegistry {
        &mut self.device_registry
    }

    /// Synthesize logical netlist from module definition
    ///
    /// Walks the module AST and builds a structured netlist representation.
    ///
    /// # Arguments
    /// * `module` - The parsed module definition from the AST
    ///
    /// # Returns
    /// Logical netlist or synthesis error
    pub fn synthesize(
        &mut self,
        module: &ModuleDefinition,
    ) -> Result<LogicalNetlist, AlignmentError> {
        let mut netlist = LogicalNetlist::new();

        // Step 1: Extract port declarations from pins field with directions
        for pin in &module.pins {
            // Convert AST PinDirection to alignment PortDirection
            let direction = match pin.direction {
                hwc_parser::PinDirection::Input => PortDirection::Input,
                hwc_parser::PinDirection::Output => PortDirection::Output,
                hwc_parser::PinDirection::Inout => PortDirection::Inout,
                hwc_parser::PinDirection::Power => PortDirection::Power,
                hwc_parser::PinDirection::Ground => PortDirection::Ground,
                hwc_parser::PinDirection::Passive => PortDirection::Inout, // Default to bidirectional
            };

            netlist.ports.push(PortInfo {
                name: pin.name.clone(),
                direction,
            });
        }

        // Step 2: Build a map of devices from AddComponent statements
        let mut devices: FxHashMap<CompactString, LogicalDevice> = FxHashMap::default();

        for statement in &module.statements {
            match statement {
                ModuleStatement::AddComponent(add) => {
                    // Extract device type (e.g., "NMOS", "PMOS")
                    let device_type_name = &add.component_type;

                    // Dynamically register device type
                    let device_type_id = self.device_registry.get_or_register(device_type_name);

                    // Extract device name
                    let device_name = add
                        .name
                        .as_ref()
                        .ok_or_else(|| AlignmentError::SynthesisError {
                            message: format!(
                                "Device of type '{}' must have a name for alignment validation",
                                device_type_name
                            )
                            .into(),
                        })?
                        .clone();

                    // Create logical device (terminals will be filled by route statements)
                    let device = LogicalDevice {
                        name: device_name.clone(),
                        device_type_id,
                        terminals: FxHashMap::default(),
                        parameters: FxHashMap::default(),
                    };

                    devices.insert(device_name, device);
                }
                _ => {
                    // Skip other statements for now (Route will be processed next)
                }
            }
        }

        // Step 3: Build terminal-to-net mapping with union-find for net merging
        // Map from "device.terminal" to net name
        let mut terminal_to_net: FxHashMap<String, String> = FxHashMap::default();
        // Union-find parent map for net merging
        let mut net_parent: FxHashMap<String, String> = FxHashMap::default();

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
                // Extract from and to references
                // IMPORTANT: Parser convention (see parse_module_pin_reference):
                //   - "M1.drain" → component="M1", pin="drain"
                //   - "VOUT"     → component="", pin="VOUT" (net name in pin field)
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

                // Verify device references
                if from_is_device && !devices.contains_key(from_component) {
                    return Err(AlignmentError::SynthesisError {
                        message: format!(
                            "Route statement references unknown device: '{}'",
                            from_component
                        )
                        .into(),
                    });
                }
                if to_is_device && !devices.contains_key(to_component) {
                    return Err(AlignmentError::SynthesisError {
                        message: format!(
                            "Route statement references unknown device: '{}'",
                            to_component
                        )
                        .into(),
                    });
                }

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

        // Step 4: Assign final net names to device terminals
        // eprintln!($3"[DEBUG LOGICAL] Assigning final net names to {} terminals", terminal_to_net.len());
        for terminal in terminal_to_net.keys() {
            if let Some((device_name, pin_name)) = terminal.split_once('.') {
                if let Some(device) = devices.get_mut(device_name) {
                    let net_name = find_root(terminal, &mut net_parent);
                    // eprintln!($3"[DEBUG LOGICAL]   {} -> net '{}'", terminal, net_name);
                    device.terminals.insert(pin_name.into(), net_name.clone());

                    // Update net info
                    netlist
                        .nets
                        .entry(net_name.clone().into())
                        .or_insert_with(|| NetInfo::new(&net_name))
                        .connected_devices
                        .push(device_name.into());
                }
            }
        }

        // eprintln!($3"[DEBUG LOGICAL] Final logical netlist has {} nets:", netlist.nets.len());
        for _net_name in netlist.nets.keys() {
            // eprintln!($3"[DEBUG LOGICAL]   Net: {}", net_name);
        }

        // Step 5: Add all devices to netlist
        netlist.devices = devices.into_values().collect();

        Ok(netlist)
    }
}

impl Default for LogicalSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}
