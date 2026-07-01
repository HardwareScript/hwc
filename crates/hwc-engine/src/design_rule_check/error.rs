//! DRC error types with miette integration.

use crate::geometry::Point3D;
use compact_str::CompactString;

use super::types::{DrcReport, DrcViolation};

/// DRC error types with miette integration.
///
/// These errors convert DRC violations into beautiful, actionable error messages
/// with error codes, help text, and source code context.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum DrcError {
    /// P16: Dielectric Breakdown - THE FAMOUS ONE
    #[error("Clearance violation between '{net_a}' and '{net_b}'")]
    #[diagnostic(
        code(P16),
        url("https://docs.hw-script.org/errors/P16"),
        help("Physical Explanation: When two copper traces carrying different voltages get too close, electrons can jump through the air or substrate (arcing/dielectric breakdown).\n\nRequired Clearance: {required_mm:.3}mm\nActual Clearance: {actual_mm:.3}mm\nVoltage Difference: Calculate from net voltages\n\nSolution: Increase spacing between nets or reduce voltage difference.\n\nFormula: clearance = (voltage / dielectric_strength) × safety_factor")
    )]
    ClearanceViolation {
        net_a: CompactString,
        net_b: CompactString,
        actual_mm: f64,
        required_mm: f64,
        location: Point3D,
    },

    /// P21: Trace Too Thin
    #[error("Trace width violation for '{net}'")]
    #[diagnostic(
        code(P21),
        url("https://docs.hw-script.org/errors/P21"),
        help("Physical Explanation: Pushing too much current through a thin trace causes Joule heating (I²R). The trace acts like a resistor and can vaporize or melt.\n\nRequired Width: {required_mm:.3}mm (IPC-2221 formula)\nActual Width: {actual_mm:.3}mm\nCurrent: Check net current specification\n\nSolution: Increase trace width or reduce current.\n\nIPC-2221 Formula: A = (I / (k × ΔT^0.44))^(1/0.725)")
    )]
    TraceWidthViolation {
        net: CompactString,
        actual_mm: f64,
        required_mm: f64,
        location: Point3D,
    },

    /// P22: Current Density Exceeds Material Limit
    #[error("Current density violation for '{net}'")]
    #[diagnostic(
        code(P22),
        url("https://docs.hw-script.org/errors/P22"),
        help("Physical Explanation: Current density exceeds the material's maximum allowed limit, risking electromigration or trace burnout.\n\nActual Density: {actual_density_a_mm2:.2} A/mm²\nMaximum Allowed: {max_density_a_mm2:.2} A/mm²\n\nSolution: Increase trace width or reduce current.\n\nMaterial current density limit is defined in materials.hw via max_current_density.")
    )]
    CurrentDensityViolation {
        net: CompactString,
        actual_density_a_mm2: f64,
        max_density_a_mm2: f64,
        location: Point3D,
    },

    /// P31: Impedance Mismatch
    #[error("Impedance violation for '{net}'")]
    #[diagnostic(
        code(P31),
        url("https://docs.hw-script.org/errors/P31"),
        help("Physical Explanation: High-speed signals require controlled impedance to prevent reflections and signal integrity issues.\n\nTarget Impedance: {target_ohm:.1}Ω\nActual Impedance: {actual_ohm:.1}Ω\n\nSolution: Adjust trace width, substrate thickness, or dielectric constant.\n\nImpedance Formula: Z = 87/√(εr+1.41) × ln(5.98h/(0.8w+t))")
    )]
    ImpedanceViolation {
        net: CompactString,
        actual_ohm: f64,
        target_ohm: f64,
        location: Point3D,
    },

    /// P41: Via Diameter Too Small (Task 4.2)
    #[error("Via diameter violation for '{net}'")]
    #[diagnostic(
        code(P41),
        url("https://docs.hw-script.org/errors/P41"),
        help("Physical Explanation: Vias that are too small can fail during drilling or have insufficient current capacity.\n\nRequired Diameter: {required_mm:.3}mm (from profile)\nActual Diameter: {actual_mm:.3}mm\n\nSolution: Increase via diameter to meet fabrication constraints.\n\nCheck your profile definition (e.g., profiles.hw) for minimum via diameter.")
    )]
    ViaDiameterViolation {
        net: CompactString,
        actual_mm: f64,
        required_mm: f64,
        location: Point3D,
    },

    /// P42: Via Enclosure/Annular Ring Too Small (Task 4.2)
    #[error("Via enclosure violation for '{net}'")]
    #[diagnostic(
        code(P42),
        url("https://docs.hw-script.org/errors/P42"),
        help("Physical Explanation: Insufficient copper around a via (annular ring) can cause drill breakout or weak connections.\n\nRequired Annular Ring: {required_mm:.3}mm (from profile)\nActual Annular Ring: {actual_mm:.3}mm\n\nSolution: Increase pad size around via or adjust via position.\n\nCheck your profile definition (e.g., profiles.hw) for minimum annular ring.")
    )]
    EnclosureViolation {
        net: CompactString,
        actual_mm: f64,
        required_mm: f64,
        location: Point3D,
    },

    /// P47: Substrate Short Circuit (v0.1.7 TSV)
    #[error("Substrate short circuit for net '{net}'")]
    #[diagnostic(
        code(P47),
        url("https://docs.hw-script.org/errors/P47"),
        help("Physical Explanation: A conductive net (like a TSV core) is touching the silicon substrate without an insulator liner. This causes a leakage current or a total short to the bulk.\n\nMaterial: {substrate_material}\nLocation: {location}\n\nSolution: Add an insulator liner (e.g., SiO2) to your contact placement.")
    )]
    SubstrateShortCircuit {
        net: CompactString,
        substrate_material: CompactString,
        location: Point3D,
    },

    /// P48: Keep-Out Zone Violation (v0.1.7 TSV)
    #[error("Keep-out zone violation for net '{net}'")]
    #[diagnostic(
        code(P48),
        url("https://docs.hw-script.org/errors/P48"),
        help("Physical Explanation: A component or trace is placed inside the mechanical stress keep-out zone (KOZ) of a TSV. High stress in this region can cause performance degradation or cracking.\n\nReason: {reason}\nLocation: {location}\n\nSolution: Move the violating geometry further away from the TSV.")
    )]
    KozViolation {
        net: CompactString,
        location: Point3D,
        reason: CompactString,
    },

    /// P49: Drill-to-Drill Spacing Violation (v0.1.7)
    #[error("Drill clearance violation between '{via_a}' and '{via_b}'")]
    #[diagnostic(
        code(P49),
        url("https://docs.hw-script.org/errors/P49"),
        help("Physical Explanation: If two mechanical drills hit locations that are too close, the drill bit will slip and fracture as it enters the second overlapping hole, ruining the board.\n\nRequired Spacing: {required_mm:.3}mm\nActual Spacing: {actual_mm:.3}mm\nLocation: {location}\n\nSolution: Increase spacing between vias or drill holes.")
    )]
    DrillClearanceViolation {
        via_a: CompactString,
        via_b: CompactString,
        actual_mm: f64,
        required_mm: f64,
        location: Point3D,
    },
}

/// Convert DRC violations to miette errors.
///
/// This function converts the internal DRC violation types into
/// beautiful miette diagnostic errors with error codes and help text.
///
/// # Arguments
/// * `violation` - DRC violation to convert
///
/// # Returns
/// Miette diagnostic error
impl From<&DrcViolation> for DrcError {
    fn from(violation: &DrcViolation) -> Self {
        match violation {
            DrcViolation::ClearanceViolation {
                net_a,
                net_b,
                actual_nm,
                required_nm,
                location,
            } => DrcError::ClearanceViolation {
                net_a: net_a.clone(),
                net_b: net_b.clone(),
                actual_mm: *actual_nm as f64 / 1_000_000.0,
                required_mm: *required_nm as f64 / 1_000_000.0,
                location: *location,
            },
            DrcViolation::TraceWidthViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => DrcError::TraceWidthViolation {
                net: net.clone(),
                actual_mm: *actual_nm as f64 / 1_000_000.0,
                required_mm: *required_nm as f64 / 1_000_000.0,
                location: *location,
            },
            DrcViolation::CurrentDensityViolation {
                net,
                actual_density_a_mm2,
                max_density_a_mm2,
                location,
            } => DrcError::CurrentDensityViolation {
                net: net.clone(),
                actual_density_a_mm2: *actual_density_a_mm2,
                max_density_a_mm2: *max_density_a_mm2,
                location: *location,
            },
            DrcViolation::ImpedanceViolation {
                net,
                actual_ohm,
                target_ohm,
                location,
            } => DrcError::ImpedanceViolation {
                net: net.clone(),
                actual_ohm: *actual_ohm,
                target_ohm: *target_ohm,
                location: *location,
            },
            DrcViolation::ViaDiameterViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => DrcError::ViaDiameterViolation {
                net: net.clone(),
                actual_mm: *actual_nm as f64 / 1_000_000.0,
                required_mm: *required_nm as f64 / 1_000_000.0,
                location: *location,
            },
            DrcViolation::EnclosureViolation {
                net,
                actual_nm,
                required_nm,
                location,
            } => DrcError::EnclosureViolation {
                net: net.clone(),
                actual_mm: *actual_nm as f64 / 1_000_000.0,
                required_mm: *required_nm as f64 / 1_000_000.0,
                location: *location,
            },
            DrcViolation::SubstrateShortCircuit {
                net,
                substrate_material,
                location,
            } => DrcError::SubstrateShortCircuit {
                net: net.clone(),
                substrate_material: substrate_material.clone(),
                location: *location,
            },
            DrcViolation::KozViolation {
                net,
                location,
                reason,
            } => DrcError::KozViolation {
                net: net.clone(),
                location: *location,
                reason: reason.clone(),
            },
            DrcViolation::DrillClearanceViolation {
                via_a,
                via_b,
                actual_nm,
                required_nm,
                location,
            } => DrcError::DrillClearanceViolation {
                via_a: via_a.clone(),
                via_b: via_b.clone(),
                actual_mm: *actual_nm as f64 / 1_000_000.0,
                required_mm: *required_nm as f64 / 1_000_000.0,
                location: *location,
            },
        }
    }
}

/// Convert DRC violations to miette errors.
///
/// This function converts the internal DRC violation types into
/// beautiful miette diagnostic errors with error codes and help text.
///
/// # Arguments
/// * `violation` - DRC violation to convert
///
/// # Returns
/// Miette diagnostic error
pub fn violation_to_error(violation: &DrcViolation) -> DrcError {
    DrcError::from(violation)
}

/// Convert DRC report to a list of miette errors.
///
/// This function converts all violations in a DRC report into
/// beautiful miette diagnostic errors that can be displayed to the user.
///
/// # Arguments
/// * `report` - DRC report with violations
///
/// # Returns
/// Vector of miette diagnostic errors
///
/// # Examples
/// ```
/// use hwc_engine::design_rule_check::{DrcReport, DrcViolation, report_to_errors};
/// use hwc_engine::Point3D;
///
/// let mut report = DrcReport::new();
/// report.add_violation(DrcViolation::ClearanceViolation {
///     net_a: "VCC".into(),
///     net_b: "GND".into(),
///     actual_nm: 100_000,
///     required_nm: 200_000,
///     location: Point3D::new(0, 0, 0),
/// });
///
/// let errors = report_to_errors(&report);
/// assert_eq!(errors.len(), 1);
/// ```
pub fn report_to_errors(report: &DrcReport) -> Vec<DrcError> {
    report.violations.iter().map(violation_to_error).collect()
}
