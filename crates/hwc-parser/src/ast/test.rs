//! Test definition types

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement, PinReference};
use crate::lexer::Span;

/// AC frequency sweep configuration
///
/// Parsed from a contextual `ac: { ... }` block inside a test definition.
/// All keywords are parsed as identifiers (zero new lexer tokens) per the
/// HardwareScript Bloat Purge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcConfig {
    pub sweep_type: AcSweepType,
    pub points: u32,
    pub start_freq: Measurement,
    pub stop_freq: Measurement,
    pub span: Span,
}

/// AC sweep type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcSweepType {
    Decade, // dec
    Octave, // oct
    Linear, // lin
}

/// Transient analysis configuration
///
/// Parsed from a contextual `tran: { ... }` block inside a test definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranConfig {
    pub step: Measurement,
    pub stop: Measurement,
    pub start: Option<Measurement>, // Optional, defaults to 0
    pub span: Span,
}

/// Test definition: `test Name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub target_space: Option<Identifier>, // "for SpaceName"
    pub ac_config: Option<AcConfig>, // ac: { ... }
    pub tran_config: Option<TranConfig>, // tran: { ... }
    pub setup: Vec<TestAction>,
    pub execute: Vec<TestAction>,
    pub assertions: Vec<TestAssertion>,
    pub span: Span,
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
