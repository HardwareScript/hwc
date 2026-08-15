//! Core data structures for Design Rule Checking.

use crate::geometry::Point3D;
use compact_str::CompactString;
use std::fmt;

// ============================================================================
// DRC Violation Types
// ============================================================================

/// DRC violation types.
///
/// Each violation type contains detailed information for error reporting.
#[derive(Debug, Clone, PartialEq)]
pub enum DrcViolation {
    /// Clearance violation between two nets.
    ClearanceViolation {
        net_a: CompactString,
        net_b: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
    },

    /// Trace width violation (too thin for current).
    TraceWidthViolation {
        net: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
    },

    /// Current density exceeds material limit (electromigration/ampacity)
    /// **P21: Electromigration - Metal atom migration under high current density**
    ElectromigrationViolation {
        net: CompactString,
        actual_density_a_mm2: f64,
        max_density_a_mm2: f64,
        location: Point3D,
    },

    /// Thermal rise violation (I²R heating exceeds temperature budget)
    /// **P22: Self-Heating Validation**
    ThermalRiseViolation {
        net: CompactString,
        actual_temp_rise_c: f64,
        max_temp_rise_c: f64,
        power_uw: f64,
        resistance_ohms: f64,
        location: Point3D,
    },

    /// Impedance violation (wrong impedance for high-speed signal).
    ImpedanceViolation {
        net: CompactString,
        actual_ohm: f64,
        target_ohm: f64,
        location: Point3D,
    },

    /// Via diameter violation (via too small).
    /// **Task 4.2: DRC Engine**
    ViaDiameterViolation {
        net: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
    },

    /// Via enclosure/annular ring violation (insufficient copper around via).
    /// **Task 4.2: DRC Engine**
    EnclosureViolation {
        net: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
    },

    /// Substrate short circuit (conductor touching substrate without liner) (v0.1.7)
    SubstrateShortCircuit {
        net: CompactString,
        substrate_material: CompactString,
        location: Point3D,
    },

    /// Keep-out zone violation (geometry placed inside a forbidden KOZ) (v0.1.7)
    KozViolation {
        net: CompactString,
        location: Point3D,
        reason: CompactString,
    },

    /// Drill clearance violation (mechanical holes too close) (v0.1.7)
    DrillClearanceViolation {
        via_a: CompactString,
        via_b: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
    },

    /// Crosstalk/signal integrity violation (coupling capacitance exceeds budget) (v0.3.0)
    CrosstalkViolation {
        aggressor_net: CompactString,
        victim_net: CompactString,
        crosstalk_db: f64,
        max_crosstalk_db: f64,
        parallel_length_nm: i64,
        spacing_nm: i64,
        location: Point3D,
    },

    /// Junction breakdown violation (P46: Voltage exceeds junction safe limit)
    JunctionBreakdownViolation {
        net: CompactString,
        material: CompactString,
        substrate_material: CompactString,
        applied_voltage_v: f64,
        max_voltage_v: f64,
        location: Point3D,
    },
}

impl fmt::Display for DrcViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrcViolation::ClearanceViolation {
                net_a,
                net_b,
                actual_nm,
                required_nm,
                location,
            } => {
                write!(
                    f,
                    "Clearance violation between {} and {} at {}: {:.4}mm actual, {:.4}mm required",
                    net_a,
                    net_b,
                    location,
                    *actual_nm as f64 / 1_000_000.0,
                    *required_nm as f64 / 1_000_000.0
                )
            }
            DrcViolation::TraceWidthViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => {
                write!(
                    f,
                    "Trace width violation for {} at {}: {:.4}mm actual, {:.4}mm required",
                    net,
                    location,
                    *actual_nm as f64 / 1_000_000.0,
                    *required_nm as f64 / 1_000_000.0
                )
            }
            DrcViolation::ElectromigrationViolation {
                net,
                actual_density_a_mm2,
                max_density_a_mm2,
                location,
            } => {
                write!(
                    f,
                    "Electromigration violation for {} at {}: {:.2} A/mm² actual, {:.2} A/mm² max",
                    net, location, actual_density_a_mm2, max_density_a_mm2
                )
            }
            DrcViolation::ImpedanceViolation {
                net,
                actual_ohm,
                target_ohm,
                location,
            } => {
                write!(
                    f,
                    "Impedance violation for {} at {}: {:.1}Ω actual, {:.1}Ω target",
                    net, location, actual_ohm, target_ohm
                )
            }
            DrcViolation::ViaDiameterViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => {
                write!(
                    f,
                    "Via diameter violation for {} at {}: {:.4}mm actual, {:.4}mm required",
                    net,
                    location,
                    *actual_nm as f64 / 1_000_000.0,
                    *required_nm as f64 / 1_000_000.0
                )
            }
            DrcViolation::EnclosureViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => {
                write!(
                    f,
                    "Via enclosure violation for {} at {}: {:.4}mm actual, {:.4}mm required annular ring",
                    net,
                    location,
                    *actual_nm as f64 / 1_000_000.0,
                    *required_nm as f64 / 1_000_000.0
                )
            }
            DrcViolation::SubstrateShortCircuit {
                net,
                substrate_material,
                location,
            } => {
                write!(
                    f,
                    "Substrate short circuit for net {} touching {} at {}",
                    net, substrate_material, location
                )
            }
            DrcViolation::KozViolation {
                net,
                location,
                reason,
            } => {
                write!(
                    f,
                    "Keep-out zone violation for net {} at {}: {}",
                    net, location, reason
                )
            }
            DrcViolation::DrillClearanceViolation {
                via_a,
                via_b,
                actual_nm,
                required_nm,
                location,
            } => {
                write!(
                    f,
                    "Drill clearance: {} ↔ {} at {} ({:.4}mm < {:.4}mm)",
                    via_a,
                    via_b,
                    location,
                    *actual_nm as f64 / 1_000_000.0,
                    *required_nm as f64 / 1_000_000.0
                )
            }
            DrcViolation::CrosstalkViolation {
                aggressor_net,
                victim_net,
                crosstalk_db,
                max_crosstalk_db,
                parallel_length_nm,
                spacing_nm,
                location,
            } => {
                write!(
                    f,
                    "Crosstalk violation: {} → {} at {}: {:.1}dB coupling (max: {:.1}dB), {:.1}μm parallel, {:.0}nm spacing",
                    aggressor_net,
                    victim_net,
                    location,
                    crosstalk_db,
                    max_crosstalk_db,
                    *parallel_length_nm as f64 / 1_000.0,
                    spacing_nm
                )
            }
            DrcViolation::ThermalRiseViolation {
                net,
                actual_temp_rise_c,
                max_temp_rise_c,
                power_uw,
                resistance_ohms,
                location,
            } => {
                write!(
                    f,
                    "Thermal rise violation for {} at {}: ΔT={:.1}°C (max: {:.1}°C), P={:.2}μW, R={:.2}Ω",
                    net, location, actual_temp_rise_c, max_temp_rise_c, power_uw, resistance_ohms
                )
            }
            DrcViolation::JunctionBreakdownViolation {
                net,
                material,
                substrate_material,
                applied_voltage_v,
                max_voltage_v,
                location,
            } => {
                write!(
                    f,
                    "Junction breakdown violation for net {} at {}: {}-to-{} junction biased at {:.2}V (max: {:.2}V)",
                    net, location, material, substrate_material, applied_voltage_v, max_voltage_v
                )
            }
        }
    }
}

// ============================================================================
// DRC Report
// ============================================================================

/// DRC report containing all violations, warnings, and info messages.
#[derive(Debug, Clone)]
pub struct DrcReport {
    /// Critical violations that must be fixed
    pub violations: Vec<DrcViolation>,

    /// Warnings (non-critical issues)
    pub warnings: Vec<CompactString>,

    /// Informational messages
    pub info: Vec<CompactString>,
}

impl DrcReport {
    /// Create a new empty DRC report.
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        }
    }

    /// Check if the design is valid (no violations).
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    /// Add a violation to the report.
    pub fn add_violation(&mut self, violation: DrcViolation) {
        self.violations.push(violation);
    }

    /// Add a warning to the report.
    pub fn add_warning(&mut self, message: CompactString) {
        self.warnings.push(message);
    }

    /// Add an info message to the report.
    pub fn add_info(&mut self, message: CompactString) {
        self.info.push(message);
    }

    /// Format the report as a human-readable string.
    pub fn format_report(&self) -> CompactString {
        let mut output = String::new();

        if self.violations.is_empty() {
            output.push_str("✓ Design passes all DRC checks\n");
        } else {
            output.push_str(&format!(
                "✗ {} DRC violation(s) found:\n\n",
                self.violations.len()
            ));
            for (i, violation) in self.violations.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, violation));
            }
        }

        if !self.warnings.is_empty() {
            output.push_str(&format!("\n⚠ {} warning(s):\n", self.warnings.len()));
            for warning in &self.warnings {
                output.push_str(&format!("  - {}\n", warning));
            }
        }

        if !self.info.is_empty() {
            output.push_str(&format!("\nℹ {} info message(s):\n", self.info.len()));
            for info in &self.info {
                output.push_str(&format!("  - {}\n", info));
            }
        }

        output.into()
    }
}

impl Default for DrcReport {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DrcReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_report())
    }
}
