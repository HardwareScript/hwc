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
    /// **P21: Electromigration - STATIC BUDGET CHECK**
    /// Validates declared budget vs. wire capability, NOT simulated operating currents.
    ElectromigrationViolation {
        net: CompactString,
        actual_density_a_mm2: f64,
        max_density_a_mm2: f64,
        location: Point3D,
    },

    /// Thermal rise violation (I²R heating exceeds temperature budget)
    /// **P22: STATIC BUDGET CHECK - Hypothetical heating if trace carries declared budget**
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

    /// Minimum area violation (CMP peeling risk)
    /// Foundries impose minimum area constraints to prevent microscopic metal/poly
    /// slivers from being torn off during Chemical Mechanical Polishing (CMP).
    MinArea {
        net_name: String,
        material_name: String,
        actual_area_nm2: f64,
        required_area_nm2: f64,
        location: Point3D,
    },

    /// Latch-up prevention: Substrate tap is too distant from active channel (SKY130 latchup.1)
    LatchUpTapTooDistant {
        device: CompactString,
        actual_nm: i64,
        max_allowed_nm: i64,
        location: Point3D,
    },

    /// Die boundary violation: geometry extends outside declared space dimensions.
    DieBoundaryViolation {
        /// Name of the layer or element that overflows
        element: CompactString,
        /// The coordinate that overflowed
        location: Point3D,
        /// Which axis overflowed ("X" or "Y")
        axis: CompactString,
        /// Actual coordinate value in nm
        actual_nm: i64,
        /// Boundary limit in nm
        limit_nm: i64,
    },

    /// Mask enclosure / precision device rule violation (e.g. SKY130 rpm.3, rpm.5, licon.8)
    MaskRuleViolation {
        rule: CompactString,
        mask_layer: CompactString,
        target_layer: CompactString,
        actual_nm: i64,
        required_nm: i64,
        location: Point3D,
        description: CompactString,
    },

    /// Direct planar electrical short circuit between different nets on the same layer.
    NetShortViolation {
        net_a: CompactString,
        net_b: CompactString,
        element_a: CompactString,
        element_b: CompactString,
        layer: CompactString,
        overlap_nm2: i64,
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
                    "Static EM violation for {} at {}: Declared budget {:.2} A/mm², wire capability {:.2} A/mm²",
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
                // v0.2.1: Use nanometer precision for drill spacing to avoid truncation
                // (150nm shown as "0.0001mm" is misleading; show "150nm < 200nm" instead)
                write!(
                    f,
                    "Drill clearance: {} ↔ {} at {} ({}nm < {}nm)",
                    via_a,
                    via_b,
                    location,
                    actual_nm,
                    required_nm
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
                    "Static thermal violation for {} at {}: Budget ΔT={:.1}°C (max: {:.1}°C), P_budget={:.2}μW, R={:.2}Ω",
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
            DrcViolation::MinArea {
                net_name,
                material_name,
                actual_area_nm2,
                required_area_nm2,
                location,
            } => {
                // Convert nm² to μm² for human readability
                let actual_um2 = actual_area_nm2 / 1_000_000.0;
                let required_um2 = required_area_nm2 / 1_000_000.0;
                write!(
                    f,
                    "Minimum area violation for net {} ({}) at {}: {:.4}μm² actual, {:.2}μm² required (CMP peeling risk)",
                    net_name, material_name, location, actual_um2, required_um2
                )
            }
            DrcViolation::LatchUpTapTooDistant {
                device,
                actual_nm,
                max_allowed_nm,
                location,
            } => {
                write!(
                    f,
                    "Latch-up tap distance violation for device '{}' at {}: {:.2}μm actual, max allowed {:.2}μm (latchup.1)",
                    device,
                    location,
                    *actual_nm as f64 / 1_000.0,
                    *max_allowed_nm as f64 / 1_000.0
                )
            }
            DrcViolation::DieBoundaryViolation {
                element,
                location,
                axis,
                actual_nm,
                limit_nm,
            } => {
                write!(
                    f,
                    "Die boundary violation: '{}' at {} overflows {} axis by {:.4}μm (actual {:.4}μm, boundary {:.4}μm)",
                    element,
                    location,
                    axis,
                    (*actual_nm - *limit_nm).abs() as f64 / 1_000.0,
                    *actual_nm as f64 / 1_000.0,
                    *limit_nm as f64 / 1_000.0,
                )
            }
            DrcViolation::MaskRuleViolation {
                rule,
                mask_layer,
                target_layer,
                actual_nm,
                required_nm,
                location,
                description,
            } => {
                write!(
                    f,
                    "Mask rule {} violation ({} on {}) at {}: actual {}nm, required {}nm ({})",
                    rule,
                    mask_layer,
                    target_layer,
                    location,
                    actual_nm,
                    required_nm,
                    description
                )
            }
            DrcViolation::NetShortViolation {
                net_a,
                net_b,
                element_a,
                element_b,
                layer,
                overlap_nm2,
                location,
            } => {
                write!(
                    f,
                    "FATAL ELECTRICAL SHORT: Net '{}' ({}) and Net '{}' ({}) intersect on layer '{}' by {:.2} um² at {}",
                    net_a,
                    element_a,
                    net_b,
                    element_b,
                    layer,
                    (*overlap_nm2 as f64) / 1e6,
                    location
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
