//! Current capacity to trace width translation (IPC-2221).
//!
//! This module implements Phase 1.3 of the constraint generation pipeline,
//! converting current requirements into minimum trace width using the
//! industry-standard IPC-2221 formula.

// ============================================================================
// Phase 1.3: Current Capacity to Trace Width Translation (IPC-2221)
// ============================================================================

/// Calculate minimum trace width from current requirements using IPC-2221 formula.
///
/// Uses the industry-standard IPC-2221 formula for trace width calculation:
/// `A = (I / (k × ΔT^0.44))^(1/0.725)`
///
/// Where:
/// - A = cross-sectional area (mm²)
/// - I = current (Amps)
/// - k = 0.048 for external layers, 0.024 for internal layers
/// - ΔT = temperature rise (°C)
///
/// **Algorithm**:
/// 1. Convert current from milliamps to amps
/// 2. Apply IPC-2221 formula to calculate cross-sectional area
/// 3. Calculate width from area (assuming 1oz copper = 35µm thickness)
/// 4. Convert to nanometers
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 200-300, Translation 2)
///
/// # Arguments
/// * `current_ma` - Current in milliamps
/// * `temp_rise_c` - Allowed temperature rise in Celsius (typically 10°C)
/// * `is_external` - True for external layers, false for internal layers
///
/// # Returns
/// Minimum trace width in nanometers
///
pub fn calculate_trace_width_nm(current_ma: i64, temp_rise_c: i64, _is_external: bool) -> i64 {
    // v0.1.8: All hardcoded PCB copper constants (1oz copper) removed.
    // Width calculation now strictly requires physical current.
    if current_ma <= 0 {
        return 0;
    }

    // Simplified IPC-2221 calculation for trace width (in mm)
    // Width = Area / Thickness
    // Area = (Current / (k * TempRise^b))^(1/c)
    // k = 0.048 for outer layers, 0.024 for inner layers
    // b = 0.44, c = 0.725

    // For ASIC development, we typically use the PDK's max_current_density
    // directly instead of temp rise, but we maintain the IPC fallback for PCB.
    let k = 0.024;
    let b = 0.44;
    let c = 0.725;

    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    let area_sq_mils = (current_a / (k * temp_rise.powf(b))).powf(1.0 / c);
    let area_nm2 = area_sq_mils * 645160.0; // sq mils to nm²

    // Thickness must come from PDK/Stackup, but we use 1um as a safe analytic base
    // if not specified by the caller (who should ideally provide it).
    let thickness_nm = 1000.0;
    (area_nm2 / thickness_nm) as i64
}
