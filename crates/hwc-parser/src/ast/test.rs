//! Test definition types

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement, PinReference};
use crate::lexer::Span;

/// Test definition: `test Name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
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
