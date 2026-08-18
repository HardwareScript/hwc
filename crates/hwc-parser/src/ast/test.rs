//! Strongly-Typed Test Definition AST & Simulation Directives

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement, PinReference};
use crate::lexer::Span;

/// Scale type for sweeps (linear, decade, octave)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SweepScale {
    Linear,
    Decade,
    Octave,
}

/// Strongly-typed sweep definition (1D or one level of an N-dimensional sweep)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DcSweep {
    /// The net or parameter being swept (e.g. `Gate` or `M1.W`)
    pub target: SweepTarget,
    /// Starting value with physical unit (e.g. `0.0V` or `100uA`)
    pub start: Measurement,
    /// Ending value with physical unit (e.g. `1.8V` or `1.0mA`)
    pub stop: Measurement,
    /// Step increment with physical unit (e.g. `0.05V` or `10uA`)
    pub step: Measurement,
    /// Scale of sweep
    pub scale: SweepScale,
    pub span: Span,
}

/// Target of a sweep (resolved during semantic lowering, not guessed)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SweepTarget {
    /// Target net (e.g. `Gate`, `V_Bias`)
    Net(Identifier),
    /// Target component parameter (e.g. `M1.W`)
    DeviceParam {
        device: Identifier,
        param: Identifier,
    },
    /// Ambient operating temperature
    Temperature,
    /// Global circuit variable
    GlobalParam(Identifier),
}

/// DC Analysis Directive (supports arbitrary multi-dimensional nested sweeps)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DcAnalysis {
    pub sweeps: Vec<DcSweep>,
    pub span: Span,
}

/// AC Frequency Response Analysis Directive
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcAnalysis {
    pub scale: SweepScale,
    pub points: u32,
    pub start_freq: Measurement,
    pub stop_freq: Measurement,
    pub span: Span,
}

/// Transient Time-Domain Analysis Directive
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranAnalysis {
    pub step: Measurement,
    pub stop: Measurement,
    pub start: Option<Measurement>,
    pub use_initial_conditions: bool,
    pub span: Span,
}

/// Small-Signal Noise Analysis Directive
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoiseAnalysis {
    pub output_net: Identifier,
    pub ref_net: Option<Identifier>,
    pub scale: SweepScale,
    pub points: u32,
    pub start_freq: Measurement,
    pub stop_freq: Measurement,
    pub span: Span,
}

/// Operating Point Analysis Directive
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpAnalysis {
    pub span: Span,
}

/// Custom Simulation Directive (for EDA tool specific cards)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericAnalysis {
    pub name: Identifier,
    pub parameters: Vec<(Identifier, DirectiveValue)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DirectiveValue {
    Measure(Measurement),
    Ident(Identifier),
    Number(f64),
    StringLit(String),
    Nested(Vec<(Identifier, DirectiveValue)>),
}

/// Container for all first-class simulation directives
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SimulationDirective {
    Dc(DcAnalysis),
    Ac(AcAnalysis),
    Tran(TranAnalysis),
    Noise(NoiseAnalysis),
    Op(OpAnalysis),
    Generic(GenericAnalysis),
}

/// Complete Test Definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestDefinition {
    pub name: Identifier,
    pub is_exported: bool,
    pub target_space: Option<Identifier>,
    pub directives: Vec<SimulationDirective>,
    pub setup: Vec<TestAction>,
    pub execute: Vec<TestAction>,
    pub assertions: Vec<TestAssertion>,
    pub span: Span,
}

impl TestDefinition {
    pub fn dc_analyses(&self) -> impl Iterator<Item = &DcAnalysis> {
        self.directives.iter().filter_map(|d| match d {
            SimulationDirective::Dc(dc) => Some(dc),
            _ => None,
        })
    }

    pub fn ac_analysis(&self) -> Option<&AcAnalysis> {
        self.directives.iter().find_map(|d| match d {
            SimulationDirective::Ac(ac) => Some(ac),
            _ => None,
        })
    }

    pub fn tran_analysis(&self) -> Option<&TranAnalysis> {
        self.directives.iter().find_map(|d| match d {
            SimulationDirective::Tran(tran) => Some(tran),
            _ => None,
        })
    }

    pub fn noise_analysis(&self) -> Option<&NoiseAnalysis> {
        self.directives.iter().find_map(|d| match d {
            SimulationDirective::Noise(noise) => Some(noise),
            _ => None,
        })
    }
}

/// Test action (setup or execute step)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestAction {
    pub action_type: TestActionType,
    pub span: Span,
}

/// Test action types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TestActionType {
    Apply {
        voltage: Measurement,
        pin: PinReference,
    },
    Short {
        from: PinReference,
        to: PinReference,
    },
    Wait {
        duration: Measurement,
    },
}

/// Test assertion
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestAssertion {
    pub pin: PinReference,
    pub condition: TestCondition,
    pub span: Span,
}

/// Test condition types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TestCondition {
    LessThan(Measurement),
    GreaterThan(Measurement),
    Equals(Measurement),
}
