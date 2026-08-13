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
    physical_netlist: Option<&PhysicalNetlist>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    symbol_table: Option<&SymbolTable>,
) -> Result<String, String> {
    let mut stimulus = String::new();

    // Get module definition if the space implements a module
    let module_def = space_def
        .and_then(|space| space.implements_module.as_ref())
        .and_then(|module_name| symbol_table.and_then(|st| st.get_module(module_name).ok()));

    match mode {
        StimulusMode::DcOperatingPoint => {
            generate_dc_stimulus(space_def, unit_registry, physical_graph, module_def, physical_netlist, symbol_table, &mut stimulus)?;
        }
        StimulusMode::AcFrequencyResponse => {
            generate_ac_stimulus(space_def, unit_registry, physical_graph, module_def, &mut stimulus);
        }
        StimulusMode::Transient => {
            generate_transient_stimulus(space_def, unit_registry, physical_graph, module_def, &mut stimulus);
        }
    }

    Ok(stimulus)
}

/// Generate DC operating point stimulus
fn generate_dc_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    physical_netlist: Option<&PhysicalNetlist>,
    symbol_table: Option<&SymbolTable>,
    stimulus: &mut String,
) -> Result<(), String> {
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
                            return Err(format!(
                                "Failed to convert potential for net '{}': {}",
                                net_decl.name, e
                            ));
                        }
                    }
                }
            }
        }
        
        // DC Operating Point Output Termination (v0.2.1)
        // For output pins, add 0V voltage sources to ground them for DC measurement.
        // This allows current to flow through 2-terminal devices like resistors.
        // Without termination, outputs float and no DC current flows.
        //
        // Physics: A 0V DC source acts as both:
        // 1. A perfect ground connection (allows current flow)
        // 2. A current meter (ngspice tracks i(V_Out) automatically)
        //
        // This enables .measure statements like: R_actual = V_in / i(V_Out)
        if let Some(module) = module_def {
            for pin in &module.pins {
                if pin.direction == PinDirection::Output {
                    let net_name = pin.name.as_str();
                    
                    // Use physical entry node if routing exists
                    let node_name = physical_graph
                        .net_entry_points
                        .get(net_name)
                        .map(|s| s.as_str())
                        .unwrap_or(net_name);

                    stimulus.push_str(&format!(
                        "V_{} {} 0 DC 0.000\n",
                        net_name, node_name
                    ));
                }
            }
        }
    }
    
    // ZERO-MAGIC MEASUREMENT GUARD (v0.2.1):
    // Only generate DC resistance measurement (.measure R_actual) if the circuit 
    // contains physical devices whose PDK SPICE prefix is explicitly 'R' (Resistors).
    // NO string matching ("Capacitor") — pure metadata inspection of SPICE prefix.
    //
    // Policy-Compliant Fix for Audit Bug 3:
    // - Running .op on capacitors is VALID (I_DC = 0A is correct)
    // - The bug was emitting .measure dc R_actual = V / I for ALL devices
    // - Division by zero (V / 0A) for capacitors caused SPICE errors
    //
    // Fix: Only emit .measure R_actual if physical netlist contains resistive devices (SPICE prefix 'R')
    // This is metadata-driven - reads SPICE prefix from device definitions, not string matching
    if let Some(module) = module_def {
        let mut input_pins: Vec<&str> = Vec::new();
        let mut output_pins: Vec<&str> = Vec::new();
        
        for pin in &module.pins {
            match pin.direction {
                PinDirection::Input => input_pins.push(pin.name.as_str()),
                PinDirection::Output => output_pins.push(pin.name.as_str()),
                _ => {}
            }
        }
        
        // Determine if resistance measurement is valid by checking SPICE metadata
        // Only resistors (SPICE prefix 'R') support meaningful DC resistance measurements
        let has_resistive_devices = physical_netlist
            .and_then(|pn| {
                // Check if circuit contains resistive devices by querying SPICE prefix metadata
                let has_resistors = pn.devices.iter().any(|dev| {
                    // Get device type name from registry
                    pn.device_registry.get_name(dev.device_type_id)
                        .and_then(|device_type_name| {
                            // Look up device definition in symbol table to access SPICE metadata
                            // This is the ONLY policy-compliant way to classify devices
                            // NO string matching, NO hardcoded logic
                            symbol_table.and_then(|st| {
                                st.get_device(device_type_name).ok()
                                    .and_then(|device_def| device_def.spice_info.as_ref())
                                    .map(|spice_info| spice_info.prefix == 'R')
                            })
                        })
                        .unwrap_or(false)
                });
                Some(has_resistors)
            })
            .unwrap_or(false);
        
        // For each input-output pair, generate a resistance measurement
        // Assumes In1→Out1, In2→Out2, etc. (same index pairing)
        if has_resistive_devices {
            for (idx, (input, output)) in input_pins.iter().zip(output_pins.iter()).enumerate() {
                if let Some(space_def) = space_def {
                    // Find the input net voltage
                    if let Some(net_decl) = space_def.nets.iter().find(|n| n.name.as_str() == *input) {
                        if let Some(ref potential) = net_decl.potential {
                            if let Ok(voltage_mv) = potential.to_millivolts(unit_registry) {
                                let voltage_v = voltage_mv as f64 / 1000.0;
                                stimulus.push_str(&format!(
                                    ".measure dc R{}_actual param='{:.3} / abs(i(V_{}))'\n",
                                    idx + 1, voltage_v, output
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    
    stimulus.push_str(".op\n");
    Ok(())
}

/// Generate AC frequency response stimulus
///
/// Policy-compliant implementation (v0.2.1):
/// - Separate analysis file (ac.sp) for frequency response
/// - AC small-signal analysis for impedance, phase, gain measurements
/// - Compatible with capacitors, inductors, and reactive devices
/// - DC bias must be set with separate .op or DC sources
fn generate_ac_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    stimulus: &mut String,
) {
    // AC analysis requires DC bias point + AC small-signal stimulus
    // Generate DC sources for biasing (same as DC analysis)
    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(ref potential) = net_decl.potential {
                if should_generate_voltage_source(net_decl, module_def) {
                    match potential.to_millivolts(unit_registry) {
                        Ok(voltage_mv) => {
                            let net_name = net_decl.name.as_str();
                            let voltage_v = voltage_mv as f64 / 1000.0;

                            let node_name = physical_graph
                                .net_entry_points
                                .get(net_name)
                                .map(|s| s.as_str())
                                .unwrap_or(net_name);

                            // For input pins: DC bias + AC small-signal excitation
                            // For other pins: DC bias only
                            let is_input = module_def
                                .and_then(|m| m.pins.iter().find(|p| p.name.as_str() == net_name))
                                .map(|p| p.direction == PinDirection::Input)
                                .unwrap_or(false);

                            if is_input {
                                stimulus.push_str(&format!(
                                    "V_{} {} 0 DC {:.3} AC 1.0\n",
                                    net_name, node_name, voltage_v
                                ));
                            } else {
                                stimulus.push_str(&format!(
                                    "V_{} {} 0 DC {:.3}\n",
                                    net_name, node_name, voltage_v
                                ));
                            }
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

        // AC analysis output termination
        if let Some(module) = module_def {
            for pin in &module.pins {
                if pin.direction == PinDirection::Output {
                    let net_name = pin.name.as_str();
                    
                    let node_name = physical_graph
                        .net_entry_points
                        .get(net_name)
                        .map(|s| s.as_str())
                        .unwrap_or(net_name);

                    stimulus.push_str(&format!(
                        "V_{} {} 0 DC 0.000\n",
                        net_name, node_name
                    ));
                }
            }
        }
    }

    // AC frequency sweep: decade sweep from 100Hz to 100MHz, 20 points per decade
    // TODO: Make frequency range configurable via PDK profile or testbench properties
    // For now, use reasonable defaults for passive component characterization
    stimulus.push_str("* AC Small-Signal Frequency Response\n");
    stimulus.push_str(".ac dec 20 100 100MEG\n");
    stimulus.push_str("* Frequency range: 100Hz - 100MHz (decade sweep, 20 pts/decade)\n");
    stimulus.push_str("* Note: Adjust frequency range in PDK profile for RF/audio applications\n");
}

/// Generate transient analysis stimulus
fn generate_transient_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    stimulus: &mut String,
) {
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
