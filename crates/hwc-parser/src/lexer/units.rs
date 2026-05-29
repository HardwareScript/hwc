//! Core Unit System for Hardware Script v0.1.4
//!
//! The compiler only knows 4 essential units needed for geometry and safety.
//! All other units (capacitance, inductance, frequency, etc.) are defined in
//! the standard library (stdlib/units.hw).

use compact_str::CompactString;
use std::fmt;

/// Distance units - REQUIRED for voxel placement and routing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DistanceUnit {
    Millimeters,
    Centimeters,
    Micrometers,
}

impl fmt::Display for DistanceUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Millimeters => write!(f, "mm"),
            Self::Centimeters => write!(f, "cm"),
            Self::Micrometers => write!(f, "µm"),
        }
    }
}

/// Voltage units - REQUIRED for safety clearance calculations
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
