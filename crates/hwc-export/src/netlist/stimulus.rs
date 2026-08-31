//! SPICE Stimulus Generation for HardwareScript v0.3.1 (Type-Safe & Registry-Driven)

use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::space::NetClassification;
use hwc_parser::{Expression, SpaceDecl, TestDecl};
use hwc_types::UnitRegistry;
use rustc_hash::FxHashMap;

use super::types::{PhysicalNetlist, PhysicalNetlistGraph, StimulusMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output,
    InOut,
    Power,
    Ground,
}

impl PinDirection {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "input" | "in" => Some(Self::Input),
            "output" | "out" => Some(Self::Output),
            "inout" | "io" | "bidir" | "passive" => Some(Self::InOut),
            "power" | "pwr" | "supply" => Some(Self::Power),
            "ground" | "gnd" => Some(Self::Ground),
            _ => None,
        }
    }
}

/// Evaluated net property payload passed from the compiler pipeline / VM
#[derive(Debug, Clone)]
pub struct EvaluatedNetStimulus {
    pub name: CompactString,
    pub node_name: CompactString,
    pub direction: Option<PinDirection>,
    pub classification: NetClassification,
    pub potential_v: Option<f64>,
    pub current_a: Option<f64>,
}

/// Typed testbench analysis configurations
#[derive(Debug, Clone)]
pub enum TestAnalysisConfig {
    DcOperatingPoint,
    DcSweep {
        target_net: CompactString,
        start_v: f64,
        stop_v: f64,
        step_v: f64,
    },
    AcFrequencyResponse {
        scale: CompactString, // "dec", "oct", "lin"
        points_per_decade: u32,
        start_hz: f64,
        stop_hz: f64,
    },
    Transient {
        step_s: f64,
        stop_s: f64,
        waveforms: FxHashMap<CompactString, InputWaveform>,
    },
}

#[derive(Debug, Clone)]
pub enum InputWaveform {
    Dc(f64),
    Pulse {
        v_low: f64,
        v_high: f64,
        t_delay: f64,
        t_rise: f64,
        t_fall: f64,
        t_pulse: f64,
        t_period: f64,
    },
    Sine {
        v_offset: f64,
        v_amplitude: f64,
        freq_hz: f64,
    },
}

pub struct SpiceStimulusGenerator<'a> {
    pub unit_registry: &'a UnitRegistry,
}

impl<'a> SpiceStimulusGenerator<'a> {
    pub fn new(unit_registry: &'a UnitRegistry) -> Self {
        Self { unit_registry }
    }

    /// Generates clean SPICE stimulus without hardcoded defaults
    pub fn generate(
        &self,
        nets: &[EvaluatedNetStimulus],
        analysis: &TestAnalysisConfig,
    ) -> Result<String, String> {
        let mut out = String::with_capacity(512);

        match analysis {
            TestAnalysisConfig::DcOperatingPoint => {
                self.emit_dc_sources(nets, &mut out);
                self.emit_load_currents(nets, &mut out);
                out.push_str(".op\n");
            }
            TestAnalysisConfig::DcSweep {
                target_net,
                start_v,
                stop_v,
                step_v,
            } => {
                self.emit_dc_sources(nets, &mut out);
                self.emit_load_currents(nets, &mut out);
                out.push_str(&format!(
                    ".dc V_{} {:.4e} {:.4e} {:.4e}\n",
                    target_net, start_v, stop_v, step_v
                ));
            }
            TestAnalysisConfig::AcFrequencyResponse {
                scale,
                points_per_decade,
                start_hz,
                stop_hz,
            } => {
                self.emit_ac_sources(nets, &mut out);
                self.emit_load_currents(nets, &mut out);
                out.push_str("* AC Small-Signal Frequency Response (Configured via Testbench)\n");
                out.push_str(&format!(
                    ".ac {} {} {:.3e} {:.3e}\n",
                    scale, points_per_decade, start_hz, stop_hz
                ));
            }
            TestAnalysisConfig::Transient {
                step_s,
                stop_s,
                waveforms,
            } => {
                self.emit_transient_sources(nets, waveforms, *stop_s, &mut out);
                self.emit_load_currents(nets, &mut out);
                out.push_str(&format!(".tran {:.3e} {:.3e}\n", step_s, stop_s));
            }
        }

        Ok(out)
    }

    pub fn should_drive_voltage(net: &EvaluatedNetStimulus) -> bool {
        match net.direction {
            Some(PinDirection::Input) | Some(PinDirection::Power) | Some(PinDirection::Ground) => {
                true
            }
            Some(PinDirection::Output) | Some(PinDirection::InOut) => false,
            None => matches!(
                net.classification,
                NetClassification::Power
                    | NetClassification::Ground
                    | NetClassification::HighVoltage
            ),
        }
    }

    fn emit_dc_sources(&self, nets: &[EvaluatedNetStimulus], out: &mut String) {
        for net in nets {
            if Self::should_drive_voltage(net) {
                let v = net.potential_v.unwrap_or(0.0);
                out.push_str(&format!(
                    "V_{} {} 0 DC {:.4e}\n",
                    net.name, net.node_name, v
                ));
            }
        }
    }

    fn emit_ac_sources(&self, nets: &[EvaluatedNetStimulus], out: &mut String) {
        let mut first_input = true;
        for net in nets {
            if Self::should_drive_voltage(net) {
                let v = net.potential_v.unwrap_or(0.0);
                if net.direction == Some(PinDirection::Input) && first_input {
                    out.push_str(&format!(
                        "V_{} {} 0 DC {:.4e} AC 1.0\n",
                        net.name, net.node_name, v
                    ));
                    first_input = false;
                } else {
                    out.push_str(&format!(
                        "V_{} {} 0 DC {:.4e}\n",
                        net.name, net.node_name, v
                    ));
                }
            }
        }
    }

    fn emit_transient_sources(
        &self,
        nets: &[EvaluatedNetStimulus],
        waveforms: &FxHashMap<CompactString, InputWaveform>,
        _total_time_s: f64,
        out: &mut String,
    ) {
        for net in nets {
            if !Self::should_drive_voltage(net) {
                continue;
            }

            if let Some(waveform) = waveforms.get(&net.name) {
                match waveform {
                    InputWaveform::Dc(v) => {
                        out.push_str(&format!(
                            "V_{} {} 0 DC {:.4e}\n",
                            net.name, net.node_name, v
                        ));
                    }
                    InputWaveform::Pulse {
                        v_low,
                        v_high,
                        t_delay,
                        t_rise,
                        t_fall,
                        t_pulse,
                        t_period,
                    } => {
                        out.push_str(&format!(
                            "V_{} {} 0 PULSE({:.4e} {:.4e} {:.3e} {:.3e} {:.3e} {:.3e} {:.3e})\n",
                            net.name, net.node_name, v_low, v_high, t_delay, t_rise, t_fall, t_pulse, t_period
                        ));
                    }
                    InputWaveform::Sine {
                        v_offset,
                        v_amplitude,
                        freq_hz,
                    } => {
                        out.push_str(&format!(
                            "V_{} {} 0 SIN({:.4e} {:.4e} {:.3e})\n",
                            net.name, net.node_name, v_offset, v_amplitude, freq_hz
                        ));
                    }
                }
            } else {
                let v = net.potential_v.unwrap_or(0.0);
                out.push_str(&format!(
                    "V_{} {} 0 DC {:.4e}\n",
                    net.name, net.node_name, v
                ));
            }
        }
    }

    fn emit_load_currents(&self, nets: &[EvaluatedNetStimulus], out: &mut String) {
        for net in nets {
            if Self::should_drive_voltage(net) {
                continue;
            }
            if let Some(i) = net.current_a {
                if i > 0.0 {
                    out.push_str(&format!(
                        "I_load_{} {} 0 DC {:.4e}\n",
                        net.name, net.node_name, i
                    ));
                }
            }
        }
    }
}

/// Evaluates any AST Expression to base SI units using UnitRegistry and SymbolTable
pub fn eval_expr_to_si(
    expr: &Expression,
    unit_registry: &UnitRegistry,
    symbol_table: Option<&SymbolTable>,
) -> Option<f64> {
    match expr {
        Expression::Measurement { value, unit, .. } => {
            let sym = unit.to_symbol();
            unit_registry.to_base_si(*value, &sym).or_else(|| {
                unit.base_si_multiplier().map(|mul| *value * mul)
            })
        }
        Expression::Literal { value, .. } => Some(*value as f64),
        Expression::FloatLiteral { value, .. } => Some(*value),
        Expression::Variable { .. } => {
            // Constants are evaluated in the Comptime Engine (v0.3.0); the
            // symbol table no longer stores constant declarations, so a bare
            // variable reference cannot be resolved here.
            None
        }
        Expression::Binary { left, operator, right, .. } => {
            let l = eval_expr_to_si(left, unit_registry, symbol_table)?;
            let r = eval_expr_to_si(right, unit_registry, symbol_table)?;
            match operator {
                hwc_parser::BinaryOperator::Add => Some(l + r),
                hwc_parser::BinaryOperator::Subtract => Some(l - r),
                hwc_parser::BinaryOperator::Multiply => Some(l * r),
                hwc_parser::BinaryOperator::Divide => {
                    if r.abs() > 1e-15 {
                        Some(l / r)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        Expression::Unary { operator, operand, .. } => {
            let v = eval_expr_to_si(operand, unit_registry, symbol_table)?;
            match operator {
                hwc_parser::UnaryOperator::Negate => Some(-v),
                hwc_parser::UnaryOperator::Plus => Some(v),
                _ => None,
            }
        }
        Expression::Grouped { expression, .. } => {
            eval_expr_to_si(expression, unit_registry, symbol_table)
        }
        _ => None,
    }
}

fn get_param_si(
    params: &[(CompactString, Expression)],
    key: &str,
    unit_registry: &UnitRegistry,
    symbol_table: Option<&SymbolTable>,
) -> Option<f64> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, expr)| eval_expr_to_si(expr, unit_registry, symbol_table))
}

fn get_param_str<'a>(params: &'a [(CompactString, Expression)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key).and_then(|(_, expr)| match expr {
        Expression::Variable { name, .. } => Some(name.as_str()),
        Expression::StringLiteral { value, .. } => Some(value.as_str()),
        _ => None,
    })
}

pub fn generate_stimulus(
    space_def: Option<&SpaceDecl>,
    mode: StimulusMode,
    _physical_netlist: Option<&PhysicalNetlist>,
    unit_registry: &UnitRegistry,
    _physical_graph: &PhysicalNetlistGraph,
    symbol_table: Option<&SymbolTable>,
    test_def: Option<&TestDecl>,
) -> Result<String, String> {
    let module_def = space_def
        .and_then(|space| space.implements.as_ref())
        .and_then(|module_name| symbol_table.and_then(|st| st.get_module(module_name.as_str()).ok()));

    let mut evaluated_nets = Vec::new();
    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            let name = net_decl.name.clone();
            let node_name = name.clone();

            let direction = module_def
                .and_then(|m| m.pins.iter().find(|p| p.name == name))
                .and_then(|p| p.direction.as_deref())
                .and_then(PinDirection::from_str_loose);

            let classification = net_decl
                .classification()
                .and_then(|c| match c.to_ascii_lowercase().as_str() {
                    "power" => Some(NetClassification::Power),
                    "ground" => Some(NetClassification::Ground),
                    "signal" => Some(NetClassification::Signal),
                    "highvoltage" | "high_voltage" => Some(NetClassification::HighVoltage),
                    _ => None,
                })
                .unwrap_or(NetClassification::Unclassified);

            let potential_v = net_decl
                .potential()
                .and_then(|expr| eval_expr_to_si(expr, unit_registry, symbol_table));

            let current_a = net_decl
                .get_property("current")
                .and_then(|expr| eval_expr_to_si(expr, unit_registry, symbol_table));

            evaluated_nets.push(EvaluatedNetStimulus {
                name,
                node_name,
                direction,
                classification,
                potential_v,
                current_a,
            });
        }
    }

    let analysis_config = match mode {
        StimulusMode::DcOperatingPoint => {
            let dc_config = test_def.and_then(|t| t.configs.iter().find(|c| c.name == "dc"));
            if let Some(dc) = dc_config {
                let target_net = get_param_str(&dc.params, "sweep")
                    .map(CompactString::new)
                    .unwrap_or_else(|| "In".into());
                let start_v = get_param_si(&dc.params, "start", unit_registry, symbol_table).unwrap_or(0.0);
                let stop_v = get_param_si(&dc.params, "stop", unit_registry, symbol_table).unwrap_or(1.8);
                let step_v = get_param_si(&dc.params, "step", unit_registry, symbol_table).unwrap_or(0.05);
                TestAnalysisConfig::DcSweep {
                    target_net,
                    start_v,
                    stop_v,
                    step_v,
                }
            } else {
                TestAnalysisConfig::DcOperatingPoint
            }
        }
        StimulusMode::AcFrequencyResponse => {
            let ac_config = test_def
                .and_then(|t| t.configs.iter().find(|c| c.name == "ac"))
                .ok_or_else(|| "Missing 'ac:' analysis block in testbench".to_string())?;
            let start_hz = get_param_si(&ac_config.params, "start", unit_registry, symbol_table).unwrap_or(1.0);
            let stop_hz = get_param_si(&ac_config.params, "stop", unit_registry, symbol_table).unwrap_or(1e9);
            let points = get_param_si(&ac_config.params, "points", unit_registry, symbol_table).unwrap_or(10.0) as u32;
            let scale = get_param_str(&ac_config.params, "scale")
                .or_else(|| get_param_str(&ac_config.params, "sweep"))
                .map(CompactString::new)
                .unwrap_or_else(|| "dec".into());
            TestAnalysisConfig::AcFrequencyResponse {
                scale,
                points_per_decade: points,
                start_hz,
                stop_hz,
            }
        }
        StimulusMode::Transient => {
            let tran_config = test_def
                .and_then(|t| t.configs.iter().find(|c| c.name == "tran"))
                .ok_or_else(|| "Missing 'tran:' analysis block in testbench".to_string())?;
            let step_s = get_param_si(&tran_config.params, "step", unit_registry, symbol_table).unwrap_or(1e-11);
            let stop_s = get_param_si(&tran_config.params, "stop", unit_registry, symbol_table).unwrap_or(2e-7);

            let mut waveforms = FxHashMap::default();
            let mut input_idx = 0;
            for net in &evaluated_nets {
                if net.direction == Some(PinDirection::Input) {
                    let v_high = net.potential_v.unwrap_or(1.8);
                    let t_rise = 1.000e-10;
                    let t_fall = 1.000e-10;
                    let divisor = (1 << input_idx) as f64;
                    let t_period = stop_s / divisor.max(1.0);
                    let t_pulse = t_period * 0.5;
                    let t_delay = 0.0;
                    waveforms.insert(
                        net.name.clone(),
                        InputWaveform::Pulse {
                            v_low: 0.0,
                            v_high,
                            t_delay,
                            t_rise,
                            t_fall,
                            t_pulse,
                            t_period,
                        },
                    );
                    input_idx += 1;
                }
            }

            TestAnalysisConfig::Transient {
                step_s,
                stop_s,
                waveforms,
            }
        }
    };

    let generator = SpiceStimulusGenerator::new(unit_registry);
    generator.generate(&evaluated_nets, &analysis_config)
}
