//! Signal Group AST nodes
//!
//! Signal groups define collections of nets with shared electrical properties,
//! such as differential pairs, impedance-controlled traces, or bus groups.

use serde::{Deserialize, Serialize};

use super::common::Identifier;
use super::Span;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Signal group definition (v0.1.6)
///
/// v0.2.0: Supports optional `export` keyword for visibility control
///
/// Example:
/// ```hw
/// signal_group USB_Data:
///     type: differential_pair
///     target_impedance: 90Ω
///     max_length_mismatch: 0.15mm
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalGroupDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub group_type: SignalGroupType,
    pub properties: FxHashMap<CompactString, SignalGroupProperty>,
    pub span: Span,
}

/// Type of signal group
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalGroupType {
    /// Differential pair (e.g., USB, LVDS)
    DifferentialPair,
    /// Impedance-controlled single-ended trace
    ImpedanceControlled,
    /// Bus group (parallel traces with shared constraints)
    Bus,
    /// Custom type
    Custom(String),
}

/// Signal group property value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalGroupProperty {
    /// Impedance value (e.g., 90Ω, 50Ω)
    Impedance(f64), // In ohms
    /// Length mismatch tolerance (e.g., 0.15mm)
    LengthMismatch(f64), // In mm
    /// Minimum spacing between traces
    MinSpacing(f64), // In mm
    /// Maximum length
    MaxLength(f64), // In mm
    /// Custom string property
    String(String),
    /// Custom numeric property
    Number(f64),
    /// Custom boolean property
    Boolean(bool),
}
