use crate::{
    ClearanceViolation, ConnectivityViolation, EMViolation, ElectricalViolation,
    PhysicalContinuityViolation, ThermalViolation,
};
/// Error code mapping for physics violations.
///
/// Maps physics violations to Hardware Script error codes for consistent
/// error reporting across the compiler.
use compact_str::CompactString;

/// Physics error codes (matching hwc-engine/src/error_codes.rs)
///
/// # Error Code Ranges
/// - P10-P19: Clearance/voltage errors
/// - P20-P29: Electrical/thermal errors  
/// - P30-P39: Signal integrity errors
/// - P40-P49: Connectivity errors
/// - P50-P59: Device physics errors
///
/// # Connectivity Error Hierarchy (P40-P49)
///
/// The connectivity errors follow a three-layer validation architecture:
///
/// ## Layer 1: Symbolic Alignment (Name-based)
/// Checks if net names match between components and geometry.
///
/// ## Layer 2: Geometric Alignment (Box-based)
/// Checks if bounding boxes touch and materials are compatible.
/// - **P41: DISCONNECTED_NET** - Net has no physical path between geometries
/// - **P45: MATERIAL_INTERPENETRATION** - Different materials overlap on same net
///
/// ## Layer 3: Physical Continuity (Voxel-based)
/// Flood-fills through actual conductive material to verify electron flow.
/// - **P41: DISCONNECTED_NET** - Net has multiple disconnected islands (deeper check)
/// - **P42: SHORT_CIRCUIT** - Island has multiple net labels
/// - **P44: FLOATING_CONDUCTOR** - Island has no pins (electrically floating)
///
/// ## Pre-Validation (Syntax Check)
/// - **P43: UNASSIGNED_CONDUCTOR** - Conductive geometry has no net assignment
///
/// This is checked before physics validation to catch syntax errors early.
pub mod codes {
    // P10-P19: Clearance/voltage errors
    pub const DIELECTRIC_BREAKDOWN: &str = "P16";
    pub const CLEARANCE_TOO_SMALL: &str = "P18";

    // P20-P29: Electrical/thermal errors
    pub const VOLTAGE_DROP_TOO_HIGH: &str = "P20";
    pub const TRACE_TOO_THIN: &str = "P21";
    pub const COMPONENT_OVERHEATING: &str = "P22";
    pub const RESISTANCE_TOO_HIGH: &str = "P23";
    pub const TEMPERATURE_RISE_EXCEEDS_LIMIT: &str = "P24";
    pub const THERMAL_CLUSTERING: &str = "P25";

    // P30-P39: Signal integrity errors
    pub const IMPEDANCE_MISMATCH: &str = "P31";
    pub const CROSSTALK_RISK: &str = "P32";
    pub const SIGNAL_INTEGRITY_VIOLATION: &str = "P34";

    // P40-P49: Connectivity errors
    /// P41: Disconnected Net
    ///
    /// Detected by both Layer 2 (connectivity.rs) and Layer 3 (physical_continuity.rs).
    ///
    /// Layer 2: Checks if geometries with same net name don't touch.
    /// Layer 3: Checks if net has multiple disconnected conductive islands.
    pub const DISCONNECTED_NET: &str = "P41";

    /// P42: Short Circuit
    ///
    /// Detected by Layer 3 (physical_continuity.rs) only.
    ///
    /// A single conductive island has multiple net labels, indicating
    /// that different nets are physically connected (short circuit).
    pub const SHORT_CIRCUIT: &str = "P42";

    /// P43: Unassigned Conductor
    ///
    /// Detected by CLI pre-check (build.rs) before physics validation.
    ///
    /// Conductive geometry (pour or contact) has no 'net:' assignment.
    /// This is a syntax error, not a physics error.
    pub const UNASSIGNED_CONDUCTOR: &str = "P43";

    /// P44: Floating Conductor
    ///
    /// Detected by Layer 3 (physical_continuity.rs) only.
    ///
    /// A conductive island has no pins connected to it, meaning it's
    /// electrically floating and not connected to any component.
    pub const FLOATING_CONDUCTOR: &str = "P44";

    /// P45: Material Interpenetration
    ///
    /// Detected by Layer 2 (connectivity.rs) only.
    ///
    /// Two geometries with different materials occupy the same physical
    /// space (voxels), even though they're on the same net. This is
    /// physically impossible.
    pub const MATERIAL_INTERPENETRATION: &str = "P45";

    // P50-P59: Device physics errors
    pub const BULK_BIASING_VIOLATION: &str = "P51";
    pub const DEVICE_GEOMETRY_INVALID: &str = "P52";
    pub const MISSING_BULK_CONTACT: &str = "P53";
}

/// Formatted error with code and message
/// Task 5.4: Enhanced with optional source location for code snippet extraction
#[derive(Debug, Clone)]
pub struct PhysicsError {
    pub code: CompactString,
    pub message: CompactString,
    pub suggestion: Option<CompactString>,
    /// Optional source location (file path, line number) for code snippet extraction
    /// Format: "file.hw:42" or None if location is unknown
    pub source_location: Option<CompactString>,
}

impl PhysicsError {
    pub fn new(code: &str, message: CompactString) -> Self {
        Self {
            code: code.into(),
            message,
            suggestion: None,
            source_location: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: CompactString) -> Self {
        self.suggestion = Some(suggestion);
        self
    }
    
    pub fn with_source_location(mut self, file_path: &str, line_number: usize) -> Self {
        self.source_location = Some(format!("{}:{}", file_path, line_number).into());
        self
    }
}

/// Convert electrical violation to error code
pub fn electrical_to_error(violation: &ElectricalViolation) -> PhysicsError {
    match violation {
        ElectricalViolation::VoltageDrop {
            net,
            actual_mv,
            max_mv,
        } => PhysicsError::new(
            codes::VOLTAGE_DROP_TOO_HIGH,
            format!(
                "Voltage drop too high on net '{}': {:.1}mV actual, {:.1}mV max",
                net, actual_mv, max_mv
            ).into(),
        )
        .with_suggestion(format!(
            "Increase trace width or reduce trace length for net '{}'",
            net
        ).into()),
        ElectricalViolation::Resistance {
            net,
            actual_ohm,
            max_ohm,
        } => PhysicsError::new(
            codes::RESISTANCE_TOO_HIGH,
            format!(
                "Resistance too high on net '{}': {:.3}Ω actual, {:.3}Ω max",
                net, actual_ohm, max_ohm
            ).into(),
        )
        .with_suggestion(format!(
            "Use thicker traces or lower-resistivity material for net '{}'",
            net
        ).into()),
        ElectricalViolation::Ampacity {
            net,
            current_ma,
            required_width_nm,
            actual_width_nm,
        } => PhysicsError::new(
            codes::TRACE_TOO_THIN,
            format!(
                "Trace too thin for current on net '{}': {}mA requires {:.3}mm width, actual {:.3}mm",
                net,
                current_ma,
                *required_width_nm as f64 / 1_000_000.0,
                *actual_width_nm as f64 / 1_000_000.0
            ).into(),
        )
        .with_suggestion(format!(
            "Increase trace width to {:.3}mm for net '{}' (IPC-2221 requirement)",
            *required_width_nm as f64 / 1_000_000.0,
            net
        ).into()),
    }
}

/// Convert thermal violation to error code
pub fn thermal_to_error(violation: &ThermalViolation) -> PhysicsError {
    match violation {
        ThermalViolation::TemperatureRise {
            net,
            actual_rise_c,
            max_rise_c,
        } => PhysicsError::new(
            codes::TEMPERATURE_RISE_EXCEEDS_LIMIT,
            format!(
                "Temperature rise exceeds limit on net '{}': {:.1}°C actual, {:.1}°C max",
                net, actual_rise_c, max_rise_c
            )
            .into(),
        )
        .with_suggestion(
            format!("Increase trace width or add thermal vias for net '{}'", net).into(),
        ),
        ThermalViolation::MaxTemperature {
            net,
            actual_temp_c,
            max_temp_c,
        } => PhysicsError::new(
            codes::COMPONENT_OVERHEATING,
            format!(
                "Component overheating on net '{}': {:.1}°C actual, {:.1}°C max",
                net, actual_temp_c, max_temp_c
            )
            .into(),
        )
        .with_suggestion(
            format!(
                "Add heatsink or improve thermal management for net '{}'",
                net
            )
            .into(),
        ),
        ThermalViolation::ThermalClustering {
            nets,
            combined_power_mw,
            distance_nm,
        } => PhysicsError::new(
            codes::THERMAL_CLUSTERING,
            format!(
                "Thermal clustering detected: nets {:?} dissipate {:.1}mW within {:.3}mm",
                nets,
                combined_power_mw,
                *distance_nm as f64 / 1_000_000.0
            )
            .into(),
        )
        .with_suggestion("Increase spacing between high-power traces or add thermal vias".into()),
    }
}

/// Convert electromagnetic violation to error code
pub fn em_to_error(violation: &EMViolation) -> PhysicsError {
    match violation {
        EMViolation::ImpedanceMismatch {
            net,
            actual_ohm,
            target_ohm,
            tolerance_percent,
        } => PhysicsError::new(
            codes::IMPEDANCE_MISMATCH,
            format!(
                "Impedance mismatch on net '{}': {:.1}Ω actual, {:.1}Ω target (±{}%)",
                net, actual_ohm, target_ohm, tolerance_percent
            )
            .into(),
        )
        .with_suggestion(
            format!(
                "Adjust trace width or dielectric height to achieve {:.1}Ω impedance on net '{}'",
                target_ohm, net
            )
            .into(),
        ),
        EMViolation::Crosstalk {
            net_a,
            net_b,
            crosstalk_coefficient,
            max_coefficient,
        } => PhysicsError::new(
            codes::CROSSTALK_RISK,
            format!(
                "Crosstalk risk between nets '{}' and '{}': {:.3} coefficient, {:.3} max",
                net_a, net_b, crosstalk_coefficient, max_coefficient
            )
            .into(),
        )
        .with_suggestion(
            format!(
                "Increase spacing between nets '{}' and '{}' or route at 90° angles",
                net_a, net_b
            )
            .into(),
        ),
    }
}

/// Convert clearance violation to error code
pub fn clearance_to_error(violation: &ClearanceViolation) -> PhysicsError {
    match violation {
        ClearanceViolation::DielectricBreakdown {
            net_a,
            net_b,
            voltage_diff_mv,
            actual_clearance_nm,
            required_clearance_nm,
            material,
        } => PhysicsError::new(
            codes::DIELECTRIC_BREAKDOWN,
            format!(
                "Dielectric breakdown risk between nets '{}' and '{}': {}V difference through {} requires {:.3}mm clearance, actual {:.3}mm",
                net_a,
                net_b,
                voltage_diff_mv / 1000,
                material,
                *required_clearance_nm as f64 / 1_000_000.0,
                *actual_clearance_nm as f64 / 1_000_000.0
            ).into(),
        )
        .with_suggestion(format!(
            "Increase clearance to {:.3}mm between nets '{}' and '{}'",
            *required_clearance_nm as f64 / 1_000_000.0,
            net_a,
            net_b
        ).into()),
        ClearanceViolation::AltitudeAdjustment {
            net_a,
            net_b,
            altitude_m,
            base_clearance_nm,
            adjusted_clearance_nm,
        } => PhysicsError::new(
            codes::CLEARANCE_TOO_SMALL,
            format!(
                "Altitude adjustment required for nets '{}' and '{}': at {}m altitude, clearance must increase from {:.3}mm to {:.3}mm",
                net_a,
                net_b,
                altitude_m,
                *base_clearance_nm as f64 / 1_000_000.0,
                *adjusted_clearance_nm as f64 / 1_000_000.0
            ).into(),
        )
        .with_suggestion(format!(
            "Increase clearance to {:.3}mm for operation at {}m altitude",
            *adjusted_clearance_nm as f64 / 1_000_000.0,
            altitude_m
        ).into()),
    }
}

/// Convert connectivity violation to error code
pub fn connectivity_to_error(violation: &ConnectivityViolation) -> PhysicsError {
    match violation {
        ConnectivityViolation::DisconnectedNet {
            net_name,
            pour_a,
            pour_b,
            reason,
            smart_hint,
        } => {
            let mut error = PhysicsError::new(
                codes::DISCONNECTED_NET,
                format!(
                    "Net '{}' is disconnected: no physical path between '{}' and '{}'. {}",
                    net_name, pour_a, pour_b, reason
                )
                .into(),
            );
            if let Some(hint) = smart_hint {
                error = error.with_suggestion(hint.clone());
            }
            error
        }
        ConnectivityViolation::MaterialInterpenetration {
            net_name,
            pour_a,
            pour_b,
            material_a,
            material_b,
            overlap_location,
        } => PhysicsError::new(
            codes::MATERIAL_INTERPENETRATION,
            format!(
                "Material interpenetration on net '{}': pour '{}' ({}) overlaps with pour '{}' ({}) at {}",
                net_name, pour_a, material_a, pour_b, material_b, overlap_location
            )
            .into(),
        )
        .with_suggestion(
            "Adjust pour boundaries so they touch at edges but do not overlap in the same physical space".into(),
        ),
    }
}

/// Convert physical continuity violation to error code
pub fn physical_continuity_to_error(violation: &PhysicalContinuityViolation) -> PhysicsError {
    match violation {
        PhysicalContinuityViolation::DisconnectedNet {
            net_name,
            island_count,
            islands,
            suggested_fix,
        } => {
            let island_details = islands
                .iter()
                .map(|island| {
                    format!(
                        "Island {} at z:{}-{} ({} nodes)",
                        island.id,
                        island.bbox.min_z / 1_000_000,
                        island.bbox.max_z / 1_000_000,
                        island.node_count
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            PhysicsError::new(
                codes::DISCONNECTED_NET,
                format!(
                    "Net '{}' has {} disconnected conductive islands: {}",
                    net_name, island_count, island_details
                )
                .into(),
            )
            .with_suggestion(suggested_fix.clone())
        }
        PhysicalContinuityViolation::ShortCircuit {
            island_id,
            net_names,
            overlap_location,
            suggested_fix,
        } => PhysicsError::new(
            codes::SHORT_CIRCUIT,
            format!(
                "Short circuit detected: Island {} connects multiple nets: {} at {}",
                island_id,
                net_names.join(", "),
                overlap_location
            )
            .into(),
        )
        .with_suggestion(suggested_fix.clone()),
        PhysicalContinuityViolation::FloatingConductor {
            island_id,
            material_name,
            bbox,
            suggested_fix,
        } => PhysicsError::new(
            codes::FLOATING_CONDUCTOR,
            format!(
                "Floating conductor detected: Island {} ({}) has no pins at x:{}-{}, y:{}-{}, z:{}-{}",
                island_id,
                material_name,
                bbox.min_x / 1_000_000,
                bbox.max_x / 1_000_000,
                bbox.min_y / 1_000_000,
                bbox.max_y / 1_000_000,
                bbox.min_z / 1_000_000,
                bbox.max_z / 1_000_000
            )
            .into(),
        )
        .with_suggestion(suggested_fix.clone()),
    }
}
