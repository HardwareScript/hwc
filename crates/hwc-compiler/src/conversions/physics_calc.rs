/// Calculate minimum clearance based on voltage and dielectric strength
///
/// Implements Translation 1 from ROUTING-AND-PHYSICS.md:
/// clearance = (voltage / dielectric_strength) × safety_factor
pub fn calculate_clearance_nm(
    voltage_diff_mv: i64,
    dielectric_strength_kv_mm: f64,
    safety_factor: f64,
) -> i64 {
    let voltage_v = voltage_diff_mv as f64 / 1000.0;
    let dielectric_v_mm = dielectric_strength_kv_mm * 1000.0;
    let min_clearance_mm = voltage_v / dielectric_v_mm;
    let min_clearance_nm = min_clearance_mm * 1_000_000.0;
    (min_clearance_nm * safety_factor) as i64
}

/// Calculate trace width required for given current using IPC-2221 formula
///
/// Implements Translation 2 from ROUTING-AND-PHYSICS.md:
/// I = k × ΔT^0.44 × A^0.725
/// Solving for A: A = (I / (k × ΔT^0.44))^(1/0.725)
/// Then: width = A / thickness
pub fn calculate_trace_width_nm(
    current_ma: i64,
    temp_rise_c: i64,
    is_external: bool,
    thickness_nm: i64,
) -> i64 {
    let k = if is_external { 0.048 } else { 0.024 };

    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    let area_mils2 = (current_a / (k * temp_rise.powf(0.44))).powf(1.0 / 0.725);
    let thickness_mils = thickness_nm as f64 / 25_400.0;
    let width_mils = area_mils2 / thickness_mils;

    (width_mils * 25_400.0) as i64
}

/// Calculate minimum trace width using IPC-2221 formula with custom k-value
pub fn calculate_trace_width_nm_with_k(
    current_ma: i64,
    temp_rise_c: i64,
    k_value: f64,
    thickness_nm: i64,
) -> i64 {
    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    let area_mils2 = (current_a / (k_value * temp_rise.powf(0.44))).powf(1.0 / 0.725);
    let thickness_mils = thickness_nm as f64 / 25_400.0;
    let width_mils = area_mils2 / thickness_mils;

    (width_mils * 25_400.0) as i64
}

/// Calculate crosstalk penalty for parallel trace routing
///
/// Implements Translation 3 from ROUTING-AND-PHYSICS.md:
/// Adds exponential cost penalty when traces run parallel for too long
pub fn calculate_crosstalk_penalty(parallel_length_nm: i64, max_parallel_nm: i64) -> i64 {
    if parallel_length_nm <= max_parallel_nm {
        return 0;
    }

    let excess = parallel_length_nm - max_parallel_nm;
    let ratio = (excess * 1000) / max_parallel_nm;

    1000 + ratio + (ratio * ratio) / 2000
}
