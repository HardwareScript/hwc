//! Interface definition types

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement, PinReference};
use crate::lexer::Span;
use compact_str::CompactString;

/// Interface definition: `interface Name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub target: Option<Identifier>,
    pub bindings: Vec<Binding>,
    pub protocols: Vec<Protocol>,
    pub span: Span,
}

/// Pin binding: `Motor_PWM = DriverIC.Pin_4`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub signal_name: CompactString,
    pub pin_ref: PinReference,
    pub span: Span,
}

/// Protocol definition (e.g., I2C, SPI)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Protocol {
    pub name: CompactString,
    pub pins: Vec<ProtocolPin>,
    pub speed: Option<Measurement>,
    pub span: Span,
}

/// Protocol pin assignment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolPin {
    pub signal: CompactString,
    pub pin_ref: PinReference,
    pub span: Span,
}
