//! SPICE stimulus generation (DC operating point & transient analysis).
//!
//! Determines which nets require driving voltage sources based on module pin
//! direction, then emits the appropriate stimulus cards and analysis directive.

use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_parser::{
    ModuleDefinition, NetClassification, NetDeclaration, PinDirection, SpaceDefinition,
};
use hwc_types::UnitRegistry;

use super::types::{PhysicalNetlistGraph, StimulusMode};

/// Determines if a voltage source should be generated for a net based on module pin direction.
///
/// **Zero-Magic Rule for SPICE Stimulus Generation:**
/// - Input pins / Power nets → Generate driving source
/// - Ground pins / Ground nets → Generate ground reference (0V)
/// - Output pins / Signal nets → DO NOT generate source (circuit-determined)
///
/// **Semantic Rule:**
/// `potential:` on an output/signal net is an Expected Operating Constraint for DRC/LVS,
/// NOT an active driving source. Generating a voltage source on an output would short-circuit
/// the resistor network and destroy the simulation.
pub fn should_generate_voltage_source(
    net_decl: &NetDeclaration,
    module_def: Option<&ModuleDefinition>,
) -> bool {
    // Rule 1: Ground nets always get a ground reference (0V source)
    if net_decl.classification == NetClassification::Ground {
        return true;
    }

    // Rule 2: Power nets always get a driving source
    if net_decl.classification == NetClassification::Power {
        return true;
    }

    // Rule 3: If there's a module definition, check pin direction
    if let Some(module) = module_def {
        // Find the pin that corresponds to this net
        for pin in &module.pins {
            if pin.name.as_str() == net_decl.name.as_str() {
                return match pin.direction {
                    PinDirection::Input => true,    // Input pins need driving sources
                    PinDirection::Power => true,    // Power pins need driving sources
                    PinDirection::Ground => true,   // Ground pins need ground references
                    PinDirection::Output => false,  // Output pins are circuit-determined
                    PinDirection::Inout => true, // Bidirectional pins get sources (can be overridden later)
                    PinDirection::Passive => false, // Passive pins are circuit-determined
                };
            }
        }
    }

    // Rule 4: Artist Mode (no module) - only generate sources for power/ground classification
    // Signal nets without module context are assumed to be internal/circuit-determined
    false
}

/// Generate stimulus section based on mode
pub fn generate_stimulus(
    space_def: Option<&SpaceDefinition>,
    mode: StimulusMode,
    _physical_netlist: Option<&PhysicalNetlist>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    symbol_table: Option<&SymbolTable>,
) -> String {
    let mut stimulus = String::new();

    // Get module definition if the space implements a module
    let module_def = space_def
        .and_then(|space| space.implements_module.as_ref())
        .and_then(|module_name| symbol_table.and_then(|st| st.get_module(module_name).ok()));

    match mode {
        StimulusMode::DcOperatingPoint => {
            // Generate DC voltage sources from net declarations
            if let Some(space_def) = space_def {
                for net_decl in &space_def.nets {
                    if let Some(ref potential) = net_decl.potential {
                        // Check if this net should have a voltage source based on module pin direction
                        if should_generate_voltage_source(net_decl, module_def) {
                            match potential.to_millivolts(unit_registry) {
                                Ok(voltage_mv) => {
                                    let net_name = net_decl.name.as_str();
                                    let voltage_v = voltage_mv as f64 / 1000.0;

                                    // Use physical entry node if routing exists
                                    let node_name = physical_graph
                                        .net_entry_points
                                        .get(net_name)
                                        .map(|s| s.as_str())
                                        .unwrap_or(net_name);

                                    stimulus.push_str(&format!(
                                        "V_{} {} 0 DC {:.3}\n",
                                        net_name, node_name, voltage_v
                                    ));
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Warning: Skipping voltage source for net '{}': {}",
                                        net_decl.name, e
                                    );
                                }
                            }
                        }
                    }
                }
            }
            stimulus.push_str(".op\n");
        }
        StimulusMode::Transient => {
            // Generate voltage sources for transient analysis
            if let Some(space_def) = space_def {
                for net_decl in &space_def.nets {
                    if let Some(ref potential) = net_decl.potential {
                        // Check if this net should have a voltage source based on module pin direction
                        if should_generate_voltage_source(net_decl, module_def) {
                            match potential.to_millivolts(unit_registry) {
                                Ok(voltage_mv) => {
                                    let net_name = net_decl.name.as_str();
                                    let voltage_v = voltage_mv as f64 / 1000.0;

                                    // Use physical entry node if routing exists
                                    let node_name = physical_graph
                                        .net_entry_points
                                        .get(net_name)
                                        .map(|s| s.as_str())
                                        .unwrap_or(net_name);

                                    // Generate DC sources (not PULSE) unless explicitly configured otherwise
                                    stimulus.push_str(&format!(
                                        "V_{} {} 0 DC {:.3}\n",
                                        net_name, node_name, voltage_v
                                    ));
                                }
                                Err(e) => {
                                    eprintln!(
                                        "[STIMULUS ERROR] Failed to convert potential for net '{}': {}",
                                        net_decl.name, e
                                    );
                                }
                            }
                        }
                    }
                }
            }
            stimulus.push_str(".tran 200ns\n");

            // Don't use .plot - LTspice ignores it
            // Instead, we'll generate a .plt file separately for automatic waveform display
        }
    }

    stimulus
}
