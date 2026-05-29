//! AST for unit definitions in the standard library

use serde::{Deserialize, Serialize};

use super::common::Identifier;
use crate::lexer::Span;
use compact_str::CompactString;

/// Unit definition from standard library (units.hw) - v0.1.6
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitDefinition {
    pub name: Identifier,
    pub symbol: CompactString,
    pub aliases: Vec<CompactString>,
    pub base_si: Option<CompactString>,
    pub multiplier: Option<f64>,
    pub dimension: CompactString,
    pub description: Option<CompactString>,
    pub note: Option<CompactString>,
    pub examples: Vec<CompactString>,
    pub span: Span,
}

impl UnitDefinition {
    /// Check if a given unit string matches this definition
    pub fn matches(&self, unit_str: &str) -> bool {
        if unit_str == self.symbol {
            return true;
        }
        self.aliases.iter().any(|alias| alias == unit_str)
    }

    /// Convert a value in this unit to its base SI value
    pub fn to_base_si(&self, value: f64) -> Option<f64> {
        self.multiplier.map(|m| value * m)
    }

    /// Get the display name for this unit (symbol or first alias.into())
    pub fn display_name(&self) -> &str {
        &self.symbol
    }
}

/// Unit dimension categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitDimension {
    Capacitance,
    Inductance,
    Frequency,
    Ratio, // %, ppm
    Power,
    Energy,
    Charge, // mAh, Ah
    LogarithmicRatio,
    LogarithmicPower,
    LogarithmicVoltage,
    Time,
    Angle,
    WireGauge,
    Custom,
}

impl UnitDimension {
    /// Parse dimension string into enum variant
    pub fn parse_dimension(s: &str) -> Option<Self> {
        match s {
            "capacitance" => Some(Self::Capacitance),
            "inductance" => Some(Self::Inductance),
            "frequency" => Some(Self::Frequency),
            "ratio" => Some(Self::Ratio),
            "power" => Some(Self::Power),
            "energy" => Some(Self::Energy),
            "charge" => Some(Self::Charge),
            "logarithmic_ratio" => Some(Self::LogarithmicRatio),
            "logarithmic_power" => Some(Self::LogarithmicPower),
            "logarithmic_voltage" => Some(Self::LogarithmicVoltage),
            "time" => Some(Self::Time),
            "angle" => Some(Self::Angle),
            "wire_gauge" => Some(Self::WireGauge),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}
