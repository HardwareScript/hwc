//! Dielectric breakdown to clearance translation.
//!
//! This module implements Phase 1.2 of the constraint generation pipeline,
//! converting voltage differences and material properties into geometric
//! clearance requirements.

// ============================================================================
// Phase 1.2: Dielectric Breakdown to Clearance Translation
// ============================================================================

/// Calculate minimum clearance from voltage difference and material properties.
///
/// Uses the formula: clearance = (voltage_v / dielectric_strength_v_nm) * safety_factor
///
/// **Algorithm**:
/// 1. Convert voltage from millivolts to volts
/// 2. Convert dielectric strength from kV/mm to V/nm
/// 3. Calculate minimum clearance
/// 4. Apply safety factor (typically 2×)
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 100-200, Translation 1)
///
/// # Arguments
/// * `voltage_diff_mv` - Voltage difference in millivolts
/// * `dielectric_strength_kv_mm` - Dielectric strength in kV/mm
/// * `safety_factor` - Safety multiplier (typically 2)
///
/// # Returns
/// Minimum clearance in nanometers
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::calculate_clearance_nm;
///
/// // 120V through Air (3 kV/mm) with 2× safety factor
/// let clearance = calculate_clearance_nm(120_000, 3.0, 2);
/// assert_eq!(clearance, 80_000);  // 0.08mm
///
/// // 120V through FR4 (20 kV/mm) with 2× safety factor
/// let clearance = calculate_clearance_nm(120_000, 20.0, 2);
/// assert_eq!(clearance, 12_000);  // 0.012mm
/// ```
pub fn calculate_clearance_nm(
    voltage_diff_mv: i64,
    dielectric_strength_kv_mm: f64,
    safety_factor: i64,
) -> i64 {
    // Convert voltage from millivolts to volts
    let voltage_v = voltage_diff_mv as f64 / 1000.0;

    // Convert dielectric strength from kV/mm to V/nm
    // kV/mm = 1000 V/mm = 1000 V / 1_000_000 nm = 0.001 V/nm
    let dielectric_v_nm = (dielectric_strength_kv_mm * 1000.0) / 1_000_000.0;

    // Calculate minimum clearance: voltage / dielectric_strength
    let min_clearance_nm = voltage_v / dielectric_v_nm;

    // Apply safety factor
    (min_clearance_nm * safety_factor as f64) as i64
}


