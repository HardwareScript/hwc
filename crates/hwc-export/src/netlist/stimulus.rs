//! SPICE Stimulus Generation for HardwareScript v0.3.0

use std::collections::HashSet;
use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_parser::{
    DistanceUnit, Expression, ModuleDecl, NetDecl, SpaceDecl, TestDecl, Unit, VoltageUnit,
};
use hwc_types::UnitRegistry;

use super::types::{PhysicalNetlist, PhysicalNetlistGraph, StimulusMode};

pub fn should_generate_voltage_source(
    net_decl: &NetDecl,
    module_def: Option<&ModuleDecl>,
) -> bool {
    if let Some(module) = module_def {
        if let Some(pin) = module.pins.iter().find(|p| p.name.as_str() == net_decl.name.as_str()) {
            if let Some(ref dir) = pin.direction {
                match dir.to_lowercase().as_str() {
                    "input" | "power" | "ground" => return true,
                    "output" | "inout" => return false,
                    _ => {}
                }
            }
        }
    }

    if let Some(classification) = net_decl.classification() {
        match classification.to_lowercase().as_str() {
            "ground" | "power" | "highvoltage" | "high_voltage" => true,
            _ => false,
        }
    } else {
        false
    }
}

pub fn generate_stimulus(
    space_def: Option<&SpaceDecl>,
    mode: StimulusMode,
    physical_netlist: Option<&PhysicalNetlist>,
    unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    symbol_table: Option<&SymbolTable>,
    test_def: Option<&TestDecl>,
) -> Result<String, String> {
    let mut stimulus = String::new();

    let module_def = space_def
        .and_then(|space| space.implements.as_ref())
        .and_then(|module_name| symbol_table.and_then(|st| st.get_module(module_name.as_str()).ok()));

    match mode {
        StimulusMode::DcOperatingPoint => {
            let dc_config = test_def.and_then(|t| t.configs.iter().find(|c| c.name == "dc"));
            if let Some(dc) = dc_config {
                generate_dc_sweep_stimulus(space_def, unit_registry, physical_graph, module_def, dc, &mut stimulus)?;
            } else {
                generate_dc_op_stimulus(space_def, physical_graph, module_def, physical_netlist, symbol_table, &mut stimulus)?;
            }
        }
        StimulusMode::AcFrequencyResponse => {
            let ac_config = test_def
                .and_then(|t| t.configs.iter().find(|c| c.name == "ac"))
                .ok_or_else(|| "Missing 'ac:' analysis block in testbench".to_string())?;
            generate_ac_stimulus(space_def, physical_graph, module_def, ac_config, &mut stimulus)?;
        }
        StimulusMode::Transient => {
            let tran_config = test_def
                .and_then(|t| t.configs.iter().find(|c| c.name == "tran"))
                .ok_or_else(|| "Missing 'tran:' analysis block in testbench".to_string())?;
            generate_transient_stimulus(space_def, physical_graph, module_def, tran_config, &mut stimulus)?;
        }
    }

    Ok(stimulus)
}

fn expr_to_si(expr: &Expression) -> Option<f64> {
    match expr {
        Expression::Measurement { value, unit, .. } => {
            let scale = match unit {
                Unit::Voltage(v) => match v {
                    VoltageUnit::Volts => 1.0,
                    VoltageUnit::Millivolts => 1e-3,
                    VoltageUnit::Kilovolts => 1e3,
                },
                Unit::Distance(d) => match d {
                    DistanceUnit::Picometers => 1e-12,
                    DistanceUnit::Nanometers => 1e-9,
                    DistanceUnit::Micrometers => 1e-6,
                    DistanceUnit::Millimeters => 1e-3,
                    DistanceUnit::Centimeters => 1e-2,
                },
                Unit::Custom(s) => match s.as_str() {
                    "nV" => 1e-9,
                    "uV" => 1e-6,
                    "mV" => 1e-3,
                    "V" => 1.0,
                    "kV" => 1e3,
                    "pm" => 1e-12,
                    "nm" => 1e-9,
                    "um" | "µm" => 1e-6,
                    "mm" => 1e-3,
                    "cm" => 1e-2,
                    "m" => 1.0,
                    "fs" => 1e-15,
                    "ps" => 1e-12,
                    "ns" => 1e-9,
                    "us" | "µs" => 1e-6,
                    "ms" => 1e-3,
                    "s" => 1.0,
                    "Hz" => 1.0,
                    "kHz" => 1e3,
                    "MHz" => 1e6,
                    "GHz" => 1e9,
                    "THz" => 1e12,
                    _ => 1.0,
                },
                _ => 1.0,
            };
            Some(*value * scale)
        }
        Expression::Literal { value, .. } => Some(*value as f64),
        _ => None,
    }
}

fn get_param_si(params: &[(CompactString, Expression)], key: &str) -> Option<f64> {
    params.iter().find(|(k, _)| k == key).and_then(|(_, expr)| expr_to_si(expr))
}

fn get_param_str<'a>(params: &'a [(CompactString, Expression)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key).and_then(|(_, expr)| match expr {
        Expression::Variable { name, .. } => Some(name.as_str()),
        Expression::StringLiteral { value, .. } => Some(value.as_str()),
        _ => None,
    })
}

fn generate_dc_sweep_stimulus(
    space_def: Option<&SpaceDecl>,
    _unit_registry: &UnitRegistry,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDecl>,
    dc: &hwc_parser::TestConfig,
    stimulus: &mut String,
) -> Result<(), String> {
    let mut generated_sources = HashSet::new();

    let sweep_target = get_param_str(&dc.params, "sweep").unwrap_or("In");
    let start_v = get_param_si(&dc.params, "start").unwrap_or(0.0);
    let stop_v = get_param_si(&dc.params, "stop").unwrap_or(1.8);
    let step_v = get_param_si(&dc.params, "step").unwrap_or(0.05);

    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            let net_name = net_decl.name.as_str();
            let is_swept = net_name == sweep_target;

            if should_generate_voltage_source(net_decl, module_def) || is_swept {
                let voltage_v = if is_swept {
                    start_v
                } else if let Some(potential_expr) = net_decl.potential() {
                    expr_to_si(potential_expr).unwrap_or(0.0)
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

    if !generated_sources.contains(sweep_target) {
        let node_name = physical_graph
            .net_entry_points
            .get(sweep_target)
            .map(|s| s.as_str())
            .unwrap_or(sweep_target);
        stimulus.push_str(&format!("V_{} {} 0 DC {:.4e}\n", sweep_target, node_name, start_v));
    }

    stimulus.push_str(&format!(".dc V_{} {:.4e} {:.4e} {:.4e}\n", sweep_target, start_v, stop_v, step_v));
    Ok(())
}

fn generate_dc_op_stimulus(
    space_def: Option<&SpaceDecl>,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDecl>,
    _physical_netlist: Option<&PhysicalNetlist>,
    _symbol_table: Option<&SymbolTable>,
    stimulus: &mut String,
) -> Result<(), String> {
    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(potential_expr) = net_decl.potential() {
                if should_generate_voltage_source(net_decl, module_def) {
                    let voltage_v = expr_to_si(potential_expr).unwrap_or(0.0);
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

    stimulus.push_str(".op\n");
    Ok(())
}

fn generate_ac_stimulus(
    space_def: Option<&SpaceDecl>,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDecl>,
    ac: &hwc_parser::TestConfig,
    stimulus: &mut String,
) -> Result<(), String> {
    let primary_input = module_def
        .and_then(|m| m.pins.iter().find(|p| p.direction.as_deref() == Some("input")))
        .map(|p| p.name.as_str());

    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(potential_expr) = net_decl.potential() {
                if should_generate_voltage_source(net_decl, module_def) {
                    let voltage_v = expr_to_si(potential_expr).unwrap_or(0.0);
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

    let start_hz = get_param_si(&ac.params, "start").unwrap_or(1.0);
    let stop_hz = get_param_si(&ac.params, "stop").unwrap_or(1e9);
    let points = get_param_si(&ac.params, "points").unwrap_or(10.0) as i64;
    let scale_str = get_param_str(&ac.params, "scale").unwrap_or("dec");

    stimulus.push_str(&format!(".ac {} {} {:.3e} {:.3e}\n", scale_str, points, start_hz, stop_hz));
    Ok(())
}

fn generate_transient_stimulus(
    space_def: Option<&SpaceDecl>,
    physical_graph: &PhysicalNetlistGraph,
    module_def: Option<&ModuleDecl>,
    tran: &hwc_parser::TestConfig,
    stimulus: &mut String,
) -> Result<(), String> {
    let step_s = get_param_si(&tran.params, "step").unwrap_or(1e-11);
    let stop_s = get_param_si(&tran.params, "stop").unwrap_or(1e-8);

    let primary_input = module_def
        .and_then(|m| m.pins.iter().find(|p| p.direction.as_deref() == Some("input")))
        .map(|p| p.name.as_str());

    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            if let Some(potential_expr) = net_decl.potential() {
                if should_generate_voltage_source(net_decl, module_def) {
                    let voltage_v = expr_to_si(potential_expr).unwrap_or(0.0);
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

    stimulus.push_str(&format!(".tran {:.3e} {:.3e}\n", step_s, stop_s));
    Ok(())
}
