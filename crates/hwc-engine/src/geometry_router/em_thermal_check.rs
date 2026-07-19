use crate::geometry_router::spatial_index::IndexedSegment;

/// Electromigration parameters (Silicon).
#[derive(Clone, Debug)]
pub struct EmParams {
    /// Current density limit in A/m².
    pub j_limit: f64,
    /// Peak current in Amps (used for minimum width calculation).
    pub i_peak: f64,
}

/// Thermal parameters (IPC-2152 for PCBs).
#[derive(Clone, Debug)]
pub struct ThermalParams {
    /// Ambient temperature in °C.
    pub ambient_temp_c: f64,
    /// Maximum allowed temperature rise in °C.
    pub max_temp_rise_c: f64,
    /// Copper thickness in meters.
    pub copper_thickness_m: f64,
    /// Substrate relative permittivity.
    pub substrate_er: f64,
}

/// AC current declaration: separate RMS and peak values.
#[derive(Clone, Copy, Debug)]
pub struct AcCurrent {
    pub rms: f64,
    pub peak: f64,
}

/// Current declaration: DC (single value for RMS and peak) or AC (separate values).
#[derive(Clone, Copy, Debug)]
pub enum CurrentDeclaration {
    Dc(f64),
    Ac(AcCurrent),
}

impl CurrentDeclaration {
    #[inline]
    pub fn rms(&self) -> f64 {
        match self {
            CurrentDeclaration::Dc(v) => *v,
            CurrentDeclaration::Ac(ac) => ac.rms,
        }
    }

    #[inline]
    pub fn peak(&self) -> f64 {
        match self {
            CurrentDeclaration::Dc(v) => *v,
            CurrentDeclaration::Ac(ac) => ac.peak,
        }
    }
}

/// Convert a parser `CurrentLimitAc` to an engine `CurrentDeclaration`.
///
/// Evaluates the RMS and peak expressions to milliamps and wraps them
/// in the appropriate enum variant.
pub fn current_limit_ac_to_declaration(rms_ma: f64, peak_ma: f64) -> CurrentDeclaration {
    CurrentDeclaration::Ac(AcCurrent {
        rms: rms_ma,
        peak: peak_ma,
    })
}

/// Create a DC current declaration from a single milliamp value.
pub fn current_limit_dc(value_ma: f64) -> CurrentDeclaration {
    CurrentDeclaration::Dc(value_ma)
}

/// EM violation reported for a trace segment.
#[derive(Clone, Debug)]
pub struct EmViolation {
    pub net_id: usize,
    pub location: (i64, i64),
    pub current_density: f64,
    pub limit: f64,
    pub width_nm: i64,
    pub min_width_nm: i64,
}

/// Thermal violation reported for a trace segment.
#[derive(Clone, Debug)]
pub struct ThermalViolation {
    pub net_id: usize,
    pub location: (i64, i64),
    pub temp_rise_c: f64,
    pub max_allowed_c: f64,
}

/// Union type for DRC violations from EM/thermal checks.
#[derive(Clone, Debug)]
pub enum DrcViolation {
    Em(EmViolation),
    Thermal(ThermalViolation),
}

/// Violation flags for build-halt logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViolationFlag {
    Em,
    Thermal,
}

/// IPC-2152 empirical constant for simplified temperature rise formula.
/// Original formula uses width in mils and copper in oz:
///   ΔT = K_mils * I² / (W_mils * T_oz^0.44), K_mils ≈ 21.6 (inner layers)
/// Converted to SI (width in meters, copper in meters):
///   ΔT = K_SI * I² / (W_m * T_m^0.44)
///   K_SI = K_mils * 25.4e-6 * (35e-6)^0.44 ≈ 6.0e-6
const IPC2152_K_SI: f64 = 6.0e-6;

/// Check electromigration: if segment width < I_peak / J_limit, violation.
#[inline]
pub fn check_electromigration(segment: &IndexedSegment, params: &EmParams) -> Option<EmViolation> {
    if params.j_limit <= 0.0 || params.i_peak <= 0.0 {
        return None;
    }
    let min_width_m = params.i_peak / params.j_limit;
    let min_width_nm_f = min_width_m * 1_000_000_000.0;

    if (segment.width_nm as f64) < min_width_nm_f {
        let min_width_nm = min_width_nm_f as i64;
        // Current density at actual width: J = I / A, A ≈ width * copper_thickness
        let copper_thickness = segment.thickness_nm as f64 / 1_000_000_000.0;
        let width_m = segment.width_nm as f64 / 1_000_000_000.0;
        let cross_section = width_m * copper_thickness;
        let current_density = if cross_section > 0.0 {
            params.i_peak / cross_section
        } else {
            f64::INFINITY
        };

        Some(EmViolation {
            net_id: segment.net_id,
            location: (segment.center().x, segment.center().y),
            current_density,
            limit: params.j_limit,
            width_nm: segment.width_nm,
            min_width_nm,
        })
    } else {
        None
    }
}

/// Check IPC-2152 temperature rise.
/// Uses simplified formula: ΔT = k * I² / (W * T^0.44) with SI units.
#[inline]
pub fn check_temperature_rise(
    segment: &IndexedSegment,
    current: &CurrentDeclaration,
    params: &ThermalParams,
) -> Option<ThermalViolation> {
    let i_rms = current.rms();
    if i_rms <= 0.0 || segment.width_nm <= 0 || params.copper_thickness_m <= 0.0 {
        return None;
    }

    let width_m = segment.width_nm as f64 / 1_000_000_000.0;
    let t_pow = params.copper_thickness_m.powf(0.44);
    let delta_t = IPC2152_K_SI * i_rms * i_rms / (width_m * t_pow);

    if delta_t > params.max_temp_rise_c {
        Some(ThermalViolation {
            net_id: segment.net_id,
            location: (segment.center().x, segment.center().y),
            temp_rise_c: delta_t,
            max_allowed_c: params.max_temp_rise_c,
        })
    } else {
        None
    }
}

/// Auto-scale trace width in hotspots to meet target temperature rise.
/// Returns the minimum width in nm needed to stay below target_temp_rise_c.
#[inline]
pub fn auto_scale_width(
    segment: &IndexedSegment,
    current: &CurrentDeclaration,
    params: &ThermalParams,
    target_temp_rise_c: f64,
) -> i64 {
    let i_rms = current.rms();
    if i_rms <= 0.0 || target_temp_rise_c <= 0.0 || params.copper_thickness_m <= 0.0 {
        return segment.width_nm;
    }

    let t_pow = params.copper_thickness_m.powf(0.44);
    let min_width_m = IPC2152_K_SI * i_rms * i_rms / (target_temp_rise_c * t_pow);
    let min_width_nm = (min_width_m * 1_000_000_000.0) as i64;

    min_width_nm.max(segment.width_nm)
}

/// Batch verify EM and thermal checks across all segments.
pub fn verify_em_thermal(
    segments: &[IndexedSegment],
    current: &CurrentDeclaration,
    em_params: &EmParams,
    thermal_params: &ThermalParams,
) -> Vec<DrcViolation> {
    let mut violations = Vec::new();

    for seg in segments {
        if let Some(v) = check_electromigration(seg, em_params) {
            violations.push(DrcViolation::Em(v));
        }
        if let Some(v) = check_temperature_rise(seg, current, thermal_params) {
            violations.push(DrcViolation::Thermal(v));
        }
    }

    violations
}
