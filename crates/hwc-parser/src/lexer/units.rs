//! Core Unit System for Hardware Script v0.1.4
//!
//! The compiler only knows 4 essential units needed for geometry and safety.
//! All other units (capacitance, inductance, frequency, etc.) are defined in
//! the standard library (stdlib/units.hw).

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Distance units - REQUIRED for placement and routing
///
/// All distance units can be converted to picometers (pm), which is the
/// engine's internal coordinate representation. Maximum addressable range:
/// +/-9,220 km (i64 pm range).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DistanceUnit {
    Millimeters,
    Centimeters,
    Micrometers,
    Nanometers,
    Picometers,
}

impl fmt::Display for DistanceUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Millimeters => write!(f, "mm"),
            Self::Centimeters => write!(f, "cm"),
            Self::Micrometers => write!(f, "µm"),
            Self::Nanometers => write!(f, "nm"),
            Self::Picometers => write!(f, "pm"),
        }
    }
}

/// Voltage units - REQUIRED for safety clearance calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoltageUnit {
    Volts,
    Millivolts,
    Kilovolts,
}

impl fmt::Display for VoltageUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Volts => write!(f, "V"),
            Self::Millivolts => write!(f, "mV"),
            Self::Kilovolts => write!(f, "kV"),
        }
    }
}

/// Current units - REQUIRED for trace width calculations (IPC-2221)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurrentUnit {
    Amperes,
    Milliamperes,
    Microamperes,
}

impl fmt::Display for CurrentUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Amperes => write!(f, "A"),
            Self::Milliamperes => write!(f, "mA"),
            Self::Microamperes => write!(f, "µA"),
        }
    }
}

/// Temperature units - REQUIRED for thermal limit calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemperatureUnit {
    Celsius,
}

impl fmt::Display for TemperatureUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Celsius => write!(f, "C"),
        }
    }
}

/// Core unit enum - only essential units for compiler operations
/// All other units are handled via Custom(String) and defined in stdlib/units.hw
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Unit {
    // === CORE COMPILER UNITS (needed for geometry/safety) ===
    Distance(DistanceUnit),
    Voltage(VoltageUnit),
    Current(CurrentUnit),
    Temperature(TemperatureUnit),

    // === EVERYTHING ELSE (defined in stdlib/units.hw) ===
    /// Custom/library units - includes:
    /// - Capacitance (F, µF, nF, pF)
    /// - Inductance (H, µH, mH)
    /// - Resistance (Ω, kΩ, MΩ, GΩ)
    /// - Frequency (Hz, kHz, MHz, GHz)
    /// - Tolerance (%, ppm)
    /// - Battery (mAh, Ah)
    /// - Power (W, mW, kW)
    /// - Signal (dBm, dBµV)
    /// - Material properties (kg/m³, W/mK, Ω·m, A/mm²)
    /// - Angle (°, rad)
    /// - And any future user-defined units
    Custom(String),
}

impl Unit {
    /// Check if this unit represents a physical distance dimension.
    pub fn is_distance(&self) -> bool {
        matches!(self, Self::Distance(_))
    }

    /// Convert a measurement value in this unit to picometers (i64).
    pub fn to_picometers(&self, value: f64) -> Result<i64, String> {
        match self {
            Self::Distance(d) => Ok(d.to_picometers(value)),
            _ => Err(format!("Cannot convert unit {:?} to picometers (not a distance unit)", self)),
        }
    }

    /// Convert a measurement value in this unit to nanometers (i64).
    pub fn to_nanometers(&self, value: f64) -> Result<i64, String> {
        match self {
            Self::Distance(d) => Ok(d.to_nanometers(value).round() as i64),
            _ => Err(format!("Cannot convert unit {:?} to nanometers (not a distance unit)", self)),
        }
    }

    /// Get canonical base SI multiplier (meters, volts, amperes, etc.)
    pub fn base_si_multiplier(&self) -> Option<f64> {
        match self {
            Self::Distance(d) => Some(d.base_si_multiplier()),
            Self::Voltage(VoltageUnit::Volts) => Some(1.0),
            Self::Voltage(VoltageUnit::Millivolts) => Some(1e-3),
            Self::Voltage(VoltageUnit::Kilovolts) => Some(1e3),
            Self::Current(CurrentUnit::Amperes) => Some(1.0),
            Self::Current(CurrentUnit::Milliamperes) => Some(1e-3),
            Self::Current(CurrentUnit::Microamperes) => Some(1e-6),
            _ => None,
        }
    }

    pub fn to_symbol(&self) -> String {
        format!("{}", self)
    }

    pub fn to_spice_suffix(&self) -> Result<&'static str, String> {
        match self {
            Self::Distance(DistanceUnit::Picometers) => Ok("p"),
            Self::Distance(DistanceUnit::Nanometers) => Ok("n"),
            Self::Distance(DistanceUnit::Micrometers) => Ok("u"),
            Self::Distance(DistanceUnit::Millimeters) => Ok("m"),
            Self::Distance(DistanceUnit::Centimeters) => Ok("c"),
            Self::Voltage(VoltageUnit::Millivolts) => Ok("m"),
            Self::Voltage(VoltageUnit::Kilovolts) => Ok("k"),
            Self::Voltage(VoltageUnit::Volts) => Ok(""),
            Self::Current(CurrentUnit::Microamperes) => Ok("u"),
            Self::Current(CurrentUnit::Milliamperes) => Ok("m"),
            Self::Current(CurrentUnit::Amperes) => Ok(""),
            Self::Custom(s) => match s.as_str() {
                "fF" => Ok("f"),
                "pF" => Ok("p"),
                "nF" => Ok("n"),
                "uF" | "µF" => Ok("u"),
                "mF" => Ok("m"),
                "F" => Ok(""),
                "pH" => Ok("p"),
                "nH" => Ok("n"),
                "uH" | "µH" => Ok("u"),
                "mH" => Ok("m"),
                "H" => Ok(""),
                "mΩ" | "mohm" => Ok("m"),
                "Ω" | "ohm" => Ok(""),
                "kΩ" | "kohm" => Ok("k"),
                "MΩ" | "megohm" => Ok("meg"),
                "GΩ" | "gohm" => Ok("g"),
                "Hz" => Ok(""),
                "kHz" => Ok("k"),
                "MHz" => Ok("meg"),
                "GHz" => Ok("g"),
                _ => Ok(""),
            },
            _ => Ok(""),
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Distance(u) => write!(f, "{}", u),
            Self::Voltage(u) => write!(f, "{}", u),
            Self::Current(u) => write!(f, "{}", u),
            Self::Temperature(u) => write!(f, "{}", u),
            Self::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl DistanceUnit {
    /// Base SI multiplier in meters (1 m = 1.0)
    pub const fn base_si_multiplier(&self) -> f64 {
        match self {
            Self::Picometers => 1e-12,
            Self::Nanometers => 1e-9,
            Self::Micrometers => 1e-6,
            Self::Millimeters => 1e-3,
            Self::Centimeters => 1e-2,
        }
    }

    /// Convert a value in this unit to base SI meters (f64).
    pub fn to_base_si(&self, value: f64) -> f64 {
        value * self.base_si_multiplier()
    }

    /// Convert a value in this unit to nanometers (f64).
    pub fn to_nanometers(&self, value: f64) -> f64 {
        self.to_base_si(value) * 1_000_000_000.0
    }

    /// Convert a value in this unit to picometers (i64).
    /// This is the engine's internal coordinate representation.
    pub fn to_picometers(&self, value: f64) -> i64 {
        (self.to_base_si(value) * 1_000_000_000_000.0).round() as i64
    }
}

/// A measurement with value and unit (for Logos compatibility)
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub value: f64,
    pub unit: Unit,
}

impl Measurement {
    pub fn new(value: f64, unit: Unit) -> Self {
        Self { value, unit }
    }

    pub fn distance(value: f64, unit: DistanceUnit) -> Self {
        Self {
            value,
            unit: Unit::Distance(unit),
        }
    }

    pub fn voltage(value: f64, unit: VoltageUnit) -> Self {
        Self {
            value,
            unit: Unit::Voltage(unit),
        }
    }

    pub fn current(value: f64, unit: CurrentUnit) -> Self {
        Self {
            value,
            unit: Unit::Current(unit),
        }
    }

    pub fn temperature(value: f64, unit: TemperatureUnit) -> Self {
        Self {
            value,
            unit: Unit::Temperature(unit),
        }
    }

    pub fn custom(value: f64, unit_str: CompactString) -> Self {
        Self {
            value,
            unit: Unit::Custom(unit_str.to_string()),
        }
    }
}

impl fmt::Display for Measurement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm_to_nanometers() {
        assert_eq!(DistanceUnit::Picometers.to_nanometers(1000.0), 1.0);
        assert_eq!(DistanceUnit::Picometers.to_nanometers(1_000_000.0), 1000.0);
    }

    #[test]
    fn test_pm_to_picometers() {
        assert_eq!(DistanceUnit::Picometers.to_picometers(42.0), 42);
    }

    #[test]
    fn test_mm_to_picometers() {
        assert_eq!(DistanceUnit::Millimeters.to_picometers(1.0), 1_000_000_000);
    }

    #[test]
    fn test_cm_to_picometers() {
        assert_eq!(DistanceUnit::Centimeters.to_picometers(1.0), 10_000_000_000);
    }

    #[test]
    fn test_um_to_picometers() {
        assert_eq!(DistanceUnit::Micrometers.to_picometers(1.0), 1_000_000);
    }

    #[test]
    fn test_nm_to_picometers() {
        assert_eq!(DistanceUnit::Nanometers.to_picometers(1.0), 1_000);
    }

    #[test]
    fn test_display_pm() {
        assert_eq!(format!("{}", DistanceUnit::Picometers), "pm");
    }
}
