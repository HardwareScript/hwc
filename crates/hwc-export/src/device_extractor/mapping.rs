use compact_str::CompactString;
use hwc_compiler::alignment::netlist::{PhysicalNetlist, PortDirection, PortInfo};
use hwc_parser::ast::ModuleDefinition;
use rustc_hash::FxHashMap;
use std::collections::HashSet;

use super::DeviceExtractor;

impl<'a> DeviceExtractor<'a> {
    pub(super) fn build_terminal_to_net_mapping(
        &self,
        module: &ModuleDefinition,
    ) -> (
        FxHashMap<(CompactString, CompactString), CompactString>,
        HashSet<CompactString>,
    ) {
        use hwc_parser::ast::ModuleStatement;

        // Use union-find to properly merge nets across device-to-device connections
        let mut terminal_to_net: FxHashMap<String, String> = FxHashMap::default();
        let mut net_parent: FxHashMap<String, String> = FxHashMap::default();
        let mut all_net_names = HashSet::new();

        let explicit_module_nets: HashSet<String> = module.pins.iter().map(|p| p.name.to_string()).collect();

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
        fn union_nets(
            net1: &str,
            net2: &str,
            net_parent: &mut FxHashMap<String, String>,
            explicit_module_nets: &HashSet<String>,
        ) {
            let root1 = find_root(net1, net_parent);
            let root2 = find_root(net2, net_parent);
            if root1 != root2 {
                // ARCHITECTURAL LAW: NEVER use string heuristics like !root.contains('.') or !root.contains("__")
                // to guess if a net is user-declared. Check explicit declarations from module.pins directly.
                let is_explicit1 = explicit_module_nets.contains(&root1);
                let is_explicit2 = explicit_module_nets.contains(&root2);

                if is_explicit1 && !is_explicit2 {
                    net_parent.insert(root2, root1);
                } else if is_explicit2 && !is_explicit1 {
                    net_parent.insert(root1, root2);
                } else {
                    // Both explicit or both internal - use stable lexicographic order
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
                union_nets(&from_net, &to_net, &mut net_parent, &explicit_module_nets);
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
    pub(super) fn copy_ports_from_module(
        &self,
        module: &ModuleDefinition,
        netlist: &mut PhysicalNetlist,
    ) {
        // Build a map of module pins: name -> direction
        let mut module_pins: FxHashMap<CompactString, hwc_parser::PinDirection> =
            FxHashMap::default();
        for pin in &module.pins {
            module_pins.insert(pin.name.clone(), pin.direction);
        }

        // Collect all unique net names from devices
        let mut net_names = HashSet::new();
        for device in &netlist.devices {
            for net_name in device.terminals.values() {
                net_names.insert(net_name.clone());
            }
        }

        // v0.1.7: Also collect nets from pours and contacts (for purely passive boards)
        for pour in &self.space.pours {
            if let Some(net_name) = &pour.net {
                net_names.insert(net_name.to_string());
            }
        }
        for contact in &self.space.contacts {
            if let Some(net_name) = &contact.net {
                net_names.insert(net_name.to_string());
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
        }

        println!(
            "   ├─ Copied {} port declarations from module",
            netlist.ports.len()
        );
    }
}
