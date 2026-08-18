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

    /// P21: Electromigration - Metal Atom Migration (STATIC BUDGET CHECK)
    #[error("Static EM violation: Declared budget for '{net}' exceeds wire capability")]
    #[diagnostic(
        code(P21),
        url("https://docs.hw-script.org/errors/P21"),
        help("Physical Explanation: High current density causes electron momentum transfer that physically pushes metal atoms downstream (electron wind). Over time, this creates voids (opens) where atoms leave and hillocks (shorts) where they accumulate.\n\n⚠️  ARCHITECTURAL NOTE: This is a STATIC BUDGET CHECK, not a dynamic simulation.\nThe compiler validates: \"Can this wire geometry support its DECLARED budget?\"\nThis does NOT validate simulated operating currents (that's P21-D in post-sim sign-off).\n\nDeclared Budget Current Density: {actual_density_a_mm2:.2} A/mm²\nWire Physical Capability: {max_density_a_mm2:.2} A/mm²\n\nGoverning Equation: Black's Equation\n  MTTF = A / J^n × exp(Ea / kT)\n  Mean Time To Failure decreases exponentially with J and T\n\nSolution:\n1. Increase trace width (reduces current density)\n2. Reduce declared current budget in nets: {{ current: X }}\n3. Use material with higher EM threshold (e.g., Copper > Aluminum)\n\nTypical Limits:\n  • Aluminum: 1.0 mA/μm² (1000 A/mm²)\n  • Copper: 2.0 mA/μm² (2000 A/mm²)\n  • Polysilicon: 0.1 mA/μm² (100 A/mm²)\n\nSee: ELECTROMIGRATION-AND-THERMAL.md for the three-tier validation architecture.")
    )]
    ElectromigrationViolation {
        net: CompactString,
        actual_density_a_mm2: f64,
        max_density_a_mm2: f64,
        location: Point3D,
    },

    /// P22: Thermal Rise Violation (STATIC BUDGET CHECK)
    #[error("Static thermal violation: Declared budget for '{net}' would cause excessive heating")]
    #[diagnostic(
        code(P22),
        url("https://docs.hw-script.org/errors/P22"),
        help("Physical Explanation: High current through a resistive trace generates Joule heat (P = I²R). When this heat cannot dissipate fast enough, local temperature rises above safe limits, causing dielectric delamination, dopant drift, or thermal runaway.\n\n⚠️  ARCHITECTURAL NOTE: This is a STATIC BUDGET CHECK, not a dynamic simulation.\nThe compiler validates: \"If this trace carried its DECLARED budget, would heating be safe?\"\nThis does NOT validate simulated power dissipation (that's P22-D in post-sim sign-off).\n\nHypothetical Temperature Rise (if trace carries declared budget): {actual_temp_rise_c:.1}°C\nMaximum Allowed: {max_temp_rise_c:.1}°C\nHypothetical Power Dissipation: {power_uw:.2}μW\nTrace Resistance: {resistance_ohms:.2}Ω\nLocation: {location}\n\nSolution:\n1. Reduce declared current budget in nets: {{ current: X }}\n2. Increase trace width (lower resistance)\n3. Shorten trace length (lower resistance)\n4. Improve thermal path to substrate\n5. Use material with lower resistivity\n\nFormula: ΔT = (I_budget²×R) / (k×Surface_Area) where k = thermal conductivity\n\nSee: ELECTROMIGRATION-AND-THERMAL.md for the three-tier validation architecture.")
    )]
    ThermalRiseViolation {
        net: CompactString,
        actual_temp_rise_c: f64,
        max_temp_rise_c: f64,
        power_uw: f64,
        resistance_ohms: f64,
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

    /// P24: Crosstalk/Signal Integrity Violation (v0.3.0)
    #[error("Crosstalk violation: '{aggressor_net}' → '{victim_net}'")]
    #[diagnostic(
        code(P24),
        url("https://docs.hw-script.org/errors/P24"),
        help("Physical Explanation: When two traces run parallel for an extended distance, AC currents in the aggressor trace induce noise voltage in the victim trace via capacitive coupling (C_12). This coupling is proportional to ε₀×εᵣ×(H/S)×L.\n\nCrosstalk Coupling: {crosstalk_db:.1}dB\nMaximum Budget: {max_crosstalk_db:.1}dB\nParallel Length: {parallel_length_um:.1}μm\nSpacing: {spacing_nm}nm\nLocation: {location}\n\nSolution:\n1. Increase spacing between traces (reduces C_12)\n2. Reduce parallel run length (route on different layers)\n3. Add ground shield trace between signals\n4. Use differential signaling for noise immunity\n\nFormula: C_12 ≈ ε₀×εᵣ×(H/S)×L where H=trace height, S=spacing, L=length")
    )]
    CrosstalkViolation {
        aggressor_net: CompactString,
        victim_net: CompactString,
        crosstalk_db: f64,
        max_crosstalk_db: f64,
        parallel_length_um: f64,
        spacing_nm: i64,
        location: Point3D,
    },

    /// P46: Junction Breakdown Violation
    #[error("Junction breakdown: {applied_voltage_v:.2}V exceeds {material} rating ({max_voltage_v:.2}V)")]
    #[diagnostic(
        code(P46),
        url("https://docs.hw-script.org/errors/P46"),
        help("reduce operating voltage below {max_voltage_v:.2}V or use high-voltage material (e.g., HV_N_Well for >2V)")
    )]
    JunctionBreakdownViolation {
        net: CompactString,
        material: CompactString,
        substrate_material: CompactString,
        applied_voltage_v: f64,
        max_voltage_v: f64,
        location: Point3D,
    },

    /// Gap 2: Minimum Area Violation (CMP Peeling Prevention)
    #[error("Minimum area violation for net '{net_name}' ({material_name})")]
    #[diagnostic(
        code(GAP2),
        url("https://docs.hw-script.org/errors/GAP2"),
        help("Physical Explanation: Foundry processes impose minimum area constraints to prevent CMP (Chemical Mechanical Polishing) damage during fabrication. Microscopic slivers of metal or polysilicon can be torn off during the polishing step, causing peeling, delamination, or process defects.\n\nActual Area: {actual_area_um2:.4}μm²\nRequired Area: {required_area_um2:.2}μm²\nLocation: {location}\n\nSolution:\n1. Increase pour/pad dimensions to meet minimum area\n2. Merge nearby islands of same material/net\n3. Remove microscopic slivers from design\n\nExamples from SKY130:\n  • poly.2: Minimum polysilicon area = 0.13 μm²\n  • m1.2: Minimum metal1 area = 0.14 μm²")
    )]
    MinAreaViolation {
        net_name: String,
        material_name: String,
        actual_area_um2: f64,
        required_area_um2: f64,
        location: Point3D,
    },

    /// P47: Latch-Up Substrate Tap Distance Violation (SKY130 latchup.1)
    #[error("Latch-up tap distance violation for device '{device}'")]
    #[diagnostic(
        code(P47),
        url("https://docs.hw-script.org/errors/P47"),
        help("Physical Explanation: In CMOS semiconductor layouts, every active channel must have a substrate tap (bulk connection) within a maximum distance (e.g., 20.0µm for SKY130 latchup.1). Without this, parasitic bipolar NPN/PNP structures trigger CMOS latch-up.\n\nActual Distance: {actual_um:.2}µm\nMaximum Allowed: {max_allowed_um:.2}µm\nLocation: {location}\n\nSolution: Move Bulk_Tap closer to the channel or insert intermediate substrate tap guard rings.")
    )]
    LatchUpTapTooDistant {
        device: CompactString,
        actual_um: f64,
        max_allowed_um: f64,
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
            DrcViolation::ElectromigrationViolation {
                net,
                actual_density_a_mm2,
                max_density_a_mm2,
                location,
            } => DrcError::ElectromigrationViolation {
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
            DrcViolation::CrosstalkViolation {
                aggressor_net,
                victim_net,
                crosstalk_db,
                max_crosstalk_db,
                parallel_length_nm,
                spacing_nm,
                location,
            } => DrcError::CrosstalkViolation {
                aggressor_net: aggressor_net.clone(),
                victim_net: victim_net.clone(),
                crosstalk_db: *crosstalk_db,
                max_crosstalk_db: *max_crosstalk_db,
                parallel_length_um: *parallel_length_nm as f64 / 1_000.0,
                spacing_nm: *spacing_nm,
                location: *location,
            },
            DrcViolation::ThermalRiseViolation {
                net,
                actual_temp_rise_c,
                max_temp_rise_c,
                power_uw,
                resistance_ohms,
                location,
            } => DrcError::ThermalRiseViolation {
                net: net.clone(),
                actual_temp_rise_c: *actual_temp_rise_c,
                max_temp_rise_c: *max_temp_rise_c,
                power_uw: *power_uw,
                resistance_ohms: *resistance_ohms,
                location: *location,
            },
            DrcViolation::JunctionBreakdownViolation {
                net,
                material,
                substrate_material,
                applied_voltage_v,
                max_voltage_v,
                location,
            } => DrcError::JunctionBreakdownViolation {
                net: net.clone(),
                material: material.clone(),
                substrate_material: substrate_material.clone(),
                applied_voltage_v: *applied_voltage_v,
                max_voltage_v: *max_voltage_v,
                location: *location,
            },
            DrcViolation::MinArea {
                net_name,
                material_name,
                actual_area_nm2,
                required_area_nm2,
                location,
            } => DrcError::MinAreaViolation {
                net_name: net_name.clone(),
                material_name: material_name.clone(),
                actual_area_um2: actual_area_nm2 / 1_000_000.0,
                required_area_um2: required_area_nm2 / 1_000_000.0,
                location: *location,
            },
            DrcViolation::LatchUpTapTooDistant {
                device,
                actual_nm,
                max_allowed_nm,
                location,
            } => DrcError::LatchUpTapTooDistant {
                device: device.clone(),
                actual_um: *actual_nm as f64 / 1_000.0,
                max_allowed_um: *max_allowed_nm as f64 / 1_000.0,
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
