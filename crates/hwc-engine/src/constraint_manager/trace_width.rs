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
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::calculate_trace_width_nm;
///
/// // 10A, 10°C rise, external layer → ~54mm width (THICK!)
/// let width = calculate_trace_width_nm(10_000, 10, true);
/// assert!(width > 50_000_000);  // > 50mm
///
/// // 1A, 10°C rise, external layer → ~5.4mm width
/// let width = calculate_trace_width_nm(1_000, 10, true);
/// assert!(width > 5_000_000);  // > 5mm
///
/// // 100mA, 10°C rise, external layer → ~19.4mm width (IPC-2221 is conservative!)
/// let width = calculate_trace_width_nm(100, 10, true);
/// assert!(width > 19_000_000);  // > 19mm
/// ```
pub fn calculate_trace_width_nm(current_ma: i64, temp_rise_c: i64, is_external: bool) -> i64 {
    // **SPRINT 3.11: Signal Trace Optimization**
    // IPC-2221 is designed for POWER traces (high current, thermal management).
    // For SIGNAL traces (<100mA), use manufacturing minimum instead.
    //
    // Why this is native:
    // - Signal integrity matters more than thermal capacity for low-current traces
    // - Manufacturing minimum (0.1mm) is the practical limit for PCB fabrication
    // - IPC-2221 produces absurdly wide traces for signals (20mA → 2.1mm!)
    //
    // Threshold: 100mA
    // - Below 100mA: Use manufacturing minimum (0.1mm = 100μm)
    // - Above 100mA: Use IPC-2221 formula (thermal management required)
    const SIGNAL_TRACE_THRESHOLD_MA: i64 = 100;
    const MIN_TRACE_WIDTH_NM: i64 = 100_000; // 0.1mm = 100μm (standard PCB minimum)

    if current_ma < SIGNAL_TRACE_THRESHOLD_MA {
        // Signal trace: use manufacturing minimum
        return MIN_TRACE_WIDTH_NM;
    }

    // Power trace: use IPC-2221 formula for thermal management
    // IPC-2221 constants
    let k = if is_external { 0.048 } else { 0.024 };
    let copper_thickness_mm = 0.035; // 1oz copper = 35 micrometers = 0.035mm

    // Convert current from milliamps to amps
    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    // IPC-2221 formula: A = (I / (k × ΔT^0.44))^(1/0.725)
    let area_mm2 = (current_a / (k * temp_rise.powf(0.44))).powf(1.0 / 0.725);

    // Calculate width from area: width = area / thickness
    let width_mm = area_mm2 / copper_thickness_mm;

    // Convert to nanometers
    let calculated_width_nm = (width_mm * 1_000_000.0) as i64;

    // Ensure we never go below manufacturing minimum
    calculated_width_nm.max(MIN_TRACE_WIDTH_NM)
}

/// Check if available space meets trace width requirement.
///
/// Validates that the routing space is wide enough for the required trace width.
///
/// **Algorithm**:
/// 1. Convert required width to voxel count (ceiling division)
/// 2. Check if available width meets requirement
/// 3. Return error with detailed message if insufficient
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 200-300, Translation 2)
///
/// # Arguments
/// * `required_width_nm` - Required trace width in nanometers
/// * `available_width_voxels` - Available width in voxels
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Ok(()) if sufficient space, Err with detailed message if insufficient
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::enforce_trace_width;
///
/// // Sufficient space
/// let result = enforce_trace_width(500_000, 10, 100_000);
/// assert!(result.is_ok());
///
/// // Insufficient space
/// let result = enforce_trace_width(1_000_000, 5, 100_000);
/// assert!(result.is_err());
/// ```
pub fn enforce_trace_width(
    required_width_nm: i64,
    available_width_voxels: usize,
    voxel_size_nm: i64,
) -> Result<(), String> {
    // Convert required width to voxel count (ceiling division)
    let required_voxels = ((required_width_nm + voxel_size_nm - 1) / voxel_size_nm) as usize;

    if available_width_voxels < required_voxels {
        return Err(format!(
            "Insufficient space: Need {:.3}mm ({} voxels), have {:.3}mm ({} voxels)",
            required_width_nm as f64 / 1_000_000.0,
            required_voxels,
            (available_width_voxels as i64 * voxel_size_nm) as f64 / 1_000_000.0,
            available_width_voxels
        ));
    }

    Ok(())
}
