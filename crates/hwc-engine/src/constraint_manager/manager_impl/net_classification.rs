//! Net classification for hierarchical parallel routing.
//!
//! This module provides functions for classifying nets as "internal" (both pins
//! in the same module) or "global" (crossing module boundaries), and for building
//! interface pin lists for each module.

use crate::netlist::{NetId, NetlistArena, PinId};
use compact_str::CompactString;
use rustc_hash::FxHashSet;

/// Classification result for a single net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetClassification {
    /// Net is entirely within a single module (both pins in same module)
    Internal { module_instance: String },

    /// Net crosses module boundaries or connects to top-level components
    Global,
}

/// Result of net classification for all nets in the design.
#[derive(Debug, Clone)]
pub struct NetClassificationResult {
    /// Map from net ID to its classification
    pub classifications: rustc_hash::FxHashMap<NetId, NetClassification>,

    /// Map from module instance name to list of interface pins
    /// Interface pins are pins that connect to nets crossing module boundaries
    pub interface_pins: rustc_hash::FxHashMap<CompactString, Vec<PinId>>,
}

/// Classify all nets in the netlist as internal or global.
///
/// This is Phase 1 of the Hierarchical Parallel Routing architecture (GAP3).
///
/// # Classification Rules
/// - **Internal Net**: All pins belong to components in the same module instance
///   - Component names follow pattern: `ModuleInstance.ComponentName`
///   - Example: `MainDSP.R1` and `MainDSP.R2` are in the same module
/// - **Global Net**: Pins belong to different modules or top-level components
///   - Example: `MainDSP.Out` to `Amplifier.In` crosses module boundaries
///   - Example: `LED1.Anode` to `R1.Pin2` (top-level components)
///
/// # Arguments
/// * `netlist` - The netlist arena containing all components, pins, and nets
///
/// # Returns
/// A `NetClassificationResult` containing:
/// - Classification for each net (Internal or Global)
/// - List of interface pins for each module
///
/// # Reference
/// - `ROADMAP/v0.1.4/Gap3.md` (Phase 1: Partitioning, Net Classification)
/// - `ROADMAP/v0.1.4/GAP-IMPLEMENTATION-PLAN.md` (Section 2.1)
///
/// # Example
/// Classifies nets as either internal to a module instance or global (crossing boundaries).
pub fn classify_nets(netlist: &NetlistArena) -> NetClassificationResult {
    let mut classifications = rustc_hash::FxHashMap::default();
    let mut interface_pins: rustc_hash::FxHashMap<CompactString, FxHashSet<PinId>> =
        rustc_hash::FxHashMap::default();

    // Iterate over all nets
    for net_id in netlist.all_net_ids() {
        let net = match netlist.get_net(net_id) {
            Some(n) => n,
            None => continue,
        };

        // Get all pins in this net
        if net.pins.is_empty() {
            // Net with no pins is considered global (unconnected)
            classifications.insert(net_id, NetClassification::Global);
            continue;
        }

        // Extract module instances for all pins in this net
        let mut module_instances: FxHashSet<Option<CompactString>> = FxHashSet::default();

        for &pin_id in &net.pins {
            let pin = match netlist.get_pin(pin_id) {
                Some(p) => p,
                None => continue,
            };

            let component = match netlist.get_component(pin.parent_component) {
                Some(c) => c,
                None => continue,
            };

            // Extract module instance from component name
            // Component names follow pattern: "ModuleInstance.ComponentName"
            // Top-level components have no dot: "LED1", "R1"
            let module_instance = extract_module_instance(&component.name);
            module_instances.insert(module_instance);
        }

        // Classify the net based on module instances
        let classification = if module_instances.len() == 1 {
            // All pins in the same module (or all at top level)
            let module_opt = module_instances.iter().next().unwrap();

            match module_opt {
                Some(module_name) => {
                    // Internal to a specific module
                    NetClassification::Internal {
                        module_instance: module_name.to_string(),
                    }
                }
                None => {
                    // All pins are top-level components (no module)
                    NetClassification::Global
                }
            }
        } else {
            // Pins span multiple modules or mix of module/top-level
            NetClassification::Global
        };

        // If this is a global net, mark all module pins as interface pins
        if matches!(classification, NetClassification::Global) {
            for &pin_id in &net.pins {
                let pin = match netlist.get_pin(pin_id) {
                    Some(p) => p,
                    None => continue,
                };

                let component = match netlist.get_component(pin.parent_component) {
                    Some(c) => c,
                    None => continue,
                };

                if let Some(module_name) = extract_module_instance(&component.name) {
                    interface_pins
                        .entry(module_name)
                        .or_default()
                        .insert(pin_id);
                }
            }
        }

        classifications.insert(net_id, classification);
    }

    // Convert interface_pins from FxHashSet to Vec for easier consumption
    let interface_pins_vec: rustc_hash::FxHashMap<CompactString, Vec<PinId>> = interface_pins
        .into_iter()
        .map(|(module, pins)| (module, pins.into_iter().collect()))
        .collect();

    NetClassificationResult {
        classifications,
        interface_pins: interface_pins_vec,
    }
}

/// Extract module instance name from a component name.
///
/// Component names follow the pattern: "ModuleInstance.ComponentName"
/// Top-level components have no dot: "LED1", "R1"
///
/// # Arguments
/// * `component_name` - The full component name
///
/// # Returns
/// - `Some(module_instance)` if the component belongs to a module
/// - `None` if the component is at the top level
fn extract_module_instance(component_name: &str) -> Option<CompactString> {
    component_name
        .split_once('.')
        .map(|(module, _component)| module.into())
}
