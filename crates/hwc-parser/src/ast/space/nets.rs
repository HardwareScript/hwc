use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Measurement value with unit string (v0.2.1: Deferred unit resolution)
///
/// Stores the raw value and unit string from the source code.
/// Unit conversion happens on-demand via methods that query the unit registry.
/// This enables user-extensible units without compiler changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementValue {
    pub value: f64,
    pub unit: CompactString,
    pub span: Span,
}

impl MeasurementValue {
    /// Convert to millivolts using the unit registry
    ///
    /// Validates that the unit is a voltage dimension and converts to mV.
    /// Supports any voltage unit defined in the standard library or user code.
    pub fn to_millivolts(&self, unit_registry: &hwc_types::UnitRegistry) -> Result<i64, String> {
        // Check dimension
        let dimension = unit_registry
            .get_dimension(&self.unit)
            .ok_or_else(|| format!("Unknown unit '{}' - not found in unit registry", self.unit))?;

        if dimension != "voltage" {
            return Err(format!(
                "Expected voltage unit, got {} (dimension: {})",
                self.unit, dimension
            ));
        }

        // Convert to base SI (volts), then to millivolts
        let volts = unit_registry
            .to_base_si(self.value, &self.unit)
            .ok_or_else(|| format!("Cannot convert {} to base SI unit", self.unit))?;

        Ok((volts * 1000.0) as i64)
    }

    /// Convert to milliamperes using the unit registry
    ///
    /// Validates that the unit is a current dimension and converts to mA.
    /// Supports any current unit defined in the standard library or user code.
    pub fn to_milliamperes(&self, unit_registry: &hwc_types::UnitRegistry) -> Result<f64, String> {
        // Check dimension
        let dimension = unit_registry
            .get_dimension(&self.unit)
            .ok_or_else(|| format!("Unknown unit '{}' - not found in unit registry", self.unit))?;

        if dimension != "current" {
            return Err(format!(
                "Expected current unit, got {} (dimension: {})",
                self.unit, dimension
            ));
        }

        // Convert to base SI (amperes), then to milliamperes
        let amperes = unit_registry
            .to_base_si(self.value, &self.unit)
            .ok_or_else(|| format!("Cannot convert {} to base SI unit", self.unit))?;

        Ok(amperes * 1000.0)
    }

    /// Convert to hertz using the unit registry
    ///
    /// Validates that the unit is a frequency dimension and converts to Hz.
    /// Supports any frequency unit defined in the standard library or user code.
    pub fn to_hertz(&self, unit_registry: &hwc_types::UnitRegistry) -> Result<f64, String> {
        // Check dimension
        let dimension = unit_registry
            .get_dimension(&self.unit)
            .ok_or_else(|| format!("Unknown unit '{}' - not found in unit registry", self.unit))?;

        if dimension != "frequency" {
            return Err(format!(
                "Expected frequency unit, got {} (dimension: {})",
                self.unit, dimension
            ));
        }

        // Convert to base SI (hertz)
        unit_registry
            .to_base_si(self.value, &self.unit)
            .ok_or_else(|| format!("Cannot convert {} to base SI unit", self.unit))
    }
}

/// Net classification for physics validation (v0.1.6)
/// v0.2.1: Stores raw measurements with units, defers conversion to compiler phase
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetDeclaration {
    pub name: CompactString,
    pub classification: NetClassification,
    /// Voltage with unit (e.g., "1.8V", "3300mV")
    /// Convert using measurement.to_millivolts(registry)
    pub potential: Option<MeasurementValue>,
    /// Current with unit (e.g., "1.0nA", "500mA")
    /// Convert using measurement.to_milliamperes(registry)
    pub current: Option<MeasurementValue>,
    /// Frequency with unit (e.g., "1MHz", "50Hz")
    /// Convert using measurement.to_hertz(registry)
    pub frequency: Option<MeasurementValue>,
    pub span: Span,
}

/// Net classification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
}
