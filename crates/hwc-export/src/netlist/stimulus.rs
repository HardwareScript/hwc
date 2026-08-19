//! SPICE Stimulus Generation

use std::collections::HashSet;
use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_parser::{
    AcAnalysis, DcAnalysis, ModuleDefinition, NetClassification, NetDeclaration,
    PinDirection, SpaceDefinition, SweepScale, SweepTarget, TestDefinition,
    TranAnalysis,
};
use hwc_types::UnitRegistry;

use super::types::{PhysicalNetlistGraph, StimulusMode};

pub fn should_generate_voltage_source(
    net_decl: &NetDeclaration,
    module_def: Option<&ModuleDefinition>,
) -> bool {
    if let Some(module) = module_def {
        if let Some(pin) = module.pins.iter().find(|p| p.name.as_str() == net_decl.name.as_str()) {
            return match pin.direction {
                PinDirection::Input | PinDirection::Power | PinDirection::Ground => true,
                // Output pins generate a DC voltage source if the space explicitly declared a potential in nets
                PinDirection::Output => net_decl.potential.is_some(),
                PinDirection::Inout | PinDirection::Passive => false,
            };
        }
    }

    match net_decl.classification {
        NetClassification::Ground | NetClassification::Power | NetClassification::HighVoltage => true,
        NetClassification::Signal | NetClassification::Unclassified => false,
    }
}

pub fn generate_stimulus(
    space_def: Option<&SpaceDefinition>,
    mode: StimulusMode,
    physical_netlist: Option<&PhysicalNetlist>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    symbol_table: Option<&SymbolTable>,
    test_def: Option<&TestDefinition>,
) -> Result<String, String> {
    let mut stimulus = String::new();

    let module_def = space_def
        .and_then(|space| space.implements_module.as_ref())
        .and_then(|module_name| symbol_table.and_then(|st| st.get_module(module_name).ok()));

    match mode {
        StimulusMode::DcOperatingPoint => {
            if let Some(dc_analysis) = test_def.and_then(|t| t.dc_analyses().next()) {
                generate_dc_sweep_stimulus(space_def, unit_registry, physical_graph, module_def, dc_analysis, &mut stimulus)?;
            } else {
                generate_dc_op_stimulus(space_def, unit_registry, physical_graph, module_def, physical_netlist, symbol_table, &mut stimulus)?;
            }
        }
        StimulusMode::AcFrequencyResponse => {
            let ac_analysis = test_def
                .and_then(|t| t.ac_analysis())
                .ok_or_else(|| "Missing 'ac:' analysis block in testbench".to_string())?;
            generate_ac_stimulus(space_def, unit_registry, physical_graph, module_def, ac_analysis, &mut stimulus)?;
        }
        StimulusMode::Transient => {
            let tran_analysis = test_def
                .and_then(|t| t.tran_analysis())
                .ok_or_else(|| "Missing 'tran:' analysis block in testbench".to_string())?;
            generate_transient_stimulus(space_def, unit_registry, physical_graph, module_def, tran_analysis, &mut stimulus)?;
        }
    }

    Ok(stimulus)
}

fn generate_dc_sweep_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    dc: &DcAnalysis,
    stimulus: &mut String,
) -> Result<(), String> {
    let mut generated_sources = HashSet::new();

    // 1. Generate biased voltage sources from space net declarations
    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            let net_name = net_decl.name.as_str();
            let is_swept = dc.sweeps.iter().any(|s| match &s.target {
                SweepTarget::Net(id) => id.as_str() == net_name,
                _ => false,
            });

            if should_generate_voltage_source(net_decl, module_def) || is_swept {
                let voltage_v = if is_swept {
                    let sweep = dc.sweeps.iter().find(|s| match &s.target {
                        SweepTarget::Net(id) => id.as_str() == net_name,
                        _ => false,
                    }).unwrap();
                    measurement_to_base_si(&sweep.start, unit_registry, "voltage")?
                } else if let Some(ref potential) = net_decl.potential {
                    let mv = potential.to_millivolts(unit_registry)
                        .map_err(|e| format!("Failed to convert potential for net '{}': {}", net_name, e))?;
                    mv as f64 / 1000.0
                } else {
                    0.0
                };

                let node_name = physical_graph
                    .net_entry_points
                    .get(net_name)
                    .map(|s| s.as_str())
                    .unwrap_or(net_name);

                stimulus.push_str(&format!("V_{} {} 0 DC {:.4e}\n", net_name, node_name, voltage_v));
                generated_sources.insert(net_name.to_string());
            }
        }
    }

    // 2. Ensure every swept net has an active driving source
    for sweep in &dc.sweeps {
        if let SweepTarget::Net(id) = &sweep.target {
            let net_name = id.as_str();
            if !generated_sources.contains(net_name) {
                let start_v = measurement_to_base_si(&sweep.start, unit_registry, "voltage")?;
                let node_name = physical_graph
                    .net_entry_points
                    .get(net_name)
                    .map(|s| s.as_str())
                    .unwrap_or(net_name);

                stimulus.push_str(&format!("V_{} {} 0 DC {:.4e}\n", net_name, node_name, start_v));
                generated_sources.insert(net_name.to_string());
            }
        }
    }

    // 3. Emit standard SPICE .dc sweep card
    let mut dc_card = String::from(".dc");
    for sweep in &dc.sweeps {
        let start_v = measurement_to_base_si(&sweep.start, unit_registry, "voltage")?;
        let stop_v = measurement_to_base_si(&sweep.stop, unit_registry, "voltage")?;
        let step_v = measurement_to_base_si(&sweep.step, unit_registry, "voltage")?;

        match &sweep.target {
            SweepTarget::Net(net) => {
                let source_name = format!("V_{}", net.as_str());
                dc_card.push_str(&format!(" {} {:.4e} {:.4e} {:.4e}", source_name, start_v, stop_v, step_v));
            }
            SweepTarget::Temperature => {
                dc_card.push_str(&format!(" temp {:.2} {:.2} {:.2}", start_v, stop_v, step_v));
            }
            SweepTarget::GlobalParam(param) => {
                dc_card.push_str(&format!(" param {} {:.4e} {:.4e} {:.4e}", param.as_str(), start_v, stop_v, step_v));
            }
            SweepTarget::DeviceParam { device, param } => {
                dc_card.push_str(&format!(" {}@{} {:.4e} {:.4e} {:.4e}", device.as_str(), param.as_str(), start_v, stop_v, step_v));
            }
        }
    }
    dc_card.push('\n');
    stimulus.push_str(&dc_card);

    Ok(())
}

fn generate_dc_op_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    physical_netlist: Option<&PhysicalNetlist>,
    symbol_table: Option<&SymbolTable>,
    stimulus: &mut String,
) -> Result<(), String> {
    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(ref potential) = net_decl.potential {
                if should_generate_voltage_source(net_decl, module_def) {
                    let mv = potential.to_millivolts(unit_registry)
                        .map_err(|e| format!("Failed to convert potential for net '{}': {}", net_decl.name, e))?;
                    let voltage_v = mv as f64 / 1000.0;
                    let net_name = net_decl.name.as_str();
                    let node_name = physical_graph
                        .net_entry_points
                        .get(net_name)
                        .map(|s| s.as_str())
                        .unwrap_or(net_name);

                    stimulus.push_str(&format!("V_{} {} 0 DC {:.3}\n", net_name, node_name, voltage_v));
                }
            }
        }
    }

    // Measure resistance if resistive devices are present
    if let Some(module) = module_def {
        let has_resistive_devices = physical_netlist
            .map(|pn| {
                pn.devices.iter().any(|dev| {
                    pn.device_registry.get_name(dev.device_type_id).map_or(false, |type_name| {
                        symbol_table.map_or(false, |st| {
                            st.get_device(type_name).map_or(false, |d| {
                                d.spice_info.as_ref().map_or(false, |info| info.prefix == 'R')
                            })
                        })
                    })
                })
            })
            .unwrap_or(false);

        if has_resistive_devices {
            let input_pins: Vec<&str> = module.pins.iter().filter(|p| p.direction == PinDirection::Input).map(|p| p.name.as_str()).collect();
            let output_pins: Vec<&str> = module.pins.iter().filter(|p| p.direction == PinDirection::Output).map(|p| p.name.as_str()).collect();

            for (idx, (input, _output)) in input_pins.iter().zip(output_pins.iter()).enumerate() {
                if let Some(space_def) = space_def {
                    if let Some(net_decl) = space_def.nets.iter().find(|n| n.name.as_str() == *input) {
                        if let Some(ref potential) = net_decl.potential {
                            if let Ok(voltage_mv) = potential.to_millivolts(unit_registry) {
                                let voltage_v = voltage_mv as f64 / 1000.0;
                                stimulus.push_str(&format!(
                                    "* Resistance measurement (Ohm's Law: R = V/I)\n\
                                     .measure dc R{}_actual param='{:.3} / abs(i(V_{}))'\n",
                                    idx + 1, voltage_v, input
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

fn generate_ac_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    ac: &AcAnalysis,
    stimulus: &mut String,
) -> Result<(), String> {
    if let Some(space_def) = space_def {
        let primary_input = module_def
            .and_then(|m| m.pins.iter().find(|p| p.direction == PinDirection::Input))
            .map(|p| p.name.as_str());

        for net_decl in &space_def.nets {
            if let Some(ref potential) = net_decl.potential {
                if should_generate_voltage_source(net_decl, module_def) {
                    let mv = potential.to_millivolts(unit_registry)
                        .map_err(|e| format!("Failed to convert potential for net '{}': {}", net_decl.name, e))?;
                    let voltage_v = mv as f64 / 1000.0;
                    let net_name = net_decl.name.as_str();
                    let node_name = physical_graph
                        .net_entry_points
                        .get(net_name)
                        .map(|s| s.as_str())
                        .unwrap_or(net_name);

                    let is_primary_input = Some(net_name) == primary_input;
                    if is_primary_input {
                        stimulus.push_str(&format!("V_{} {} 0 DC {:.3} AC 1.0\n", net_name, node_name, voltage_v));
                    } else {
                        stimulus.push_str(&format!("V_{} {} 0 DC {:.3}\n", net_name, node_name, voltage_v));
                    }
                }
            }
        }
    }

    let start_hz = measurement_to_base_si(&ac.start_freq, unit_registry, "frequency")?;
    let stop_hz = measurement_to_base_si(&ac.stop_freq, unit_registry, "frequency")?;

    let scale_str = match ac.scale {
        SweepScale::Decade => "dec",
        SweepScale::Octave => "oct",
        SweepScale::Linear => "lin",
    };

    stimulus.push_str(&format!(".ac {} {} {:.3e} {:.3e}\n", scale_str, ac.points, start_hz, stop_hz));
    Ok(())
}

fn generate_transient_stimulus(
    space_def: Option<&SpaceDefinition>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDefinition>,
    tran: &TranAnalysis,
    stimulus: &mut String,
) -> Result<(), String> {
    let step_s = measurement_to_base_si(&tran.step, unit_registry, "time")?;
    let stop_s = measurement_to_base_si(&tran.stop, unit_registry, "time")?;

    let primary_input = module_def
        .and_then(|m| m.pins.iter().find(|p| p.direction == PinDirection::Input))
        .map(|p| p.name.as_str());

    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(ref potential) = net_decl.potential {
                if should_generate_voltage_source(net_decl, module_def) {
                    let mv = potential.to_millivolts(unit_registry)
                        .map_err(|e| format!("Failed to convert potential for net '{}': {}", net_decl.name, e))?;
                    let voltage_v = mv as f64 / 1000.0;
                    let net_name = net_decl.name.as_str();
                    let node_name = physical_graph
                        .net_entry_points
                        .get(net_name)
                        .map(|s| s.as_str())
                        .unwrap_or(net_name);

                    if Some(net_name) == primary_input {
                        let t_rise = step_s * 0.1;
                        let t_fall = step_s * 0.1;
                        let t_period = stop_s / 5.0;
                        let t_on = t_period * 0.45;
                        stimulus.push_str(&format!(
                            "V_{} {} 0 PULSE(0.0 {:.3} 0.0 {:.3e} {:.3e} {:.3e} {:.3e})\n",
                            net_name, node_name, voltage_v, t_rise, t_fall, t_on, t_period
                        ));
                    } else {
                        stimulus.push_str(&format!("V_{} {} 0 DC {:.3}\n", net_name, node_name, voltage_v));
                    }
                }
            }
        }
    }

    if let Some(ref start) = tran.start {
        let start_s = measurement_to_base_si(start, unit_registry, "time")?;
        stimulus.push_str(&format!(".tran {:.3e} {:.3e} {:.3e}", step_s, stop_s, start_s));
    } else {
        stimulus.push_str(&format!(".tran {:.3e} {:.3e}", step_s, stop_s));
    }

    if tran.use_initial_conditions {
        stimulus.push_str(" uic");
    }
    stimulus.push('\n');

    Ok(())
}

fn measurement_to_base_si(
    m: &hwc_parser::Measurement,
    unit_registry: &UnitRegistry,
    expected_dimension: &str,
) -> Result<f64, String> {
    let symbol = m.unit.to_symbol();
    unit_registry
        .convert_with_validation(m.value, &symbol, expected_dimension)
        .map_err(|e| format!("Measurement conversion error for '{}': {}", symbol, e))
}
