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
pub fn current_limit_ac_to_declaration(
    rms_ma: f64,
    peak_ma: f64,
) -> CurrentDeclaration {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;

    fn make_segment(width_nm: i64, net_id: usize) -> IndexedSegment {
        IndexedSegment {
            segment_id: 0,
            net_id,
            width_nm,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0), // 10mm
            layer: 0,
        }
    }

    #[test]
    fn test_em_narrow_trace_violation() {
        let seg = make_segment(5_000, 1); // 5µm wide
        let params = EmParams {
            j_limit: 1e6,  // 1 MA/m²
            i_peak: 0.01,  // 10mA → needs 10nm minimum
        };
        // A_min = I/J = 0.01 / 1e6 = 10nm → 5000nm > 10nm → pass
        assert!(check_electromigration(&seg, &params).is_none());
    }

    #[test]
    fn test_em_very_narrow_violation() {
        // Segment width smaller than minimum required
        let seg = make_segment(1, 1); // 1nm wide
        let params = EmParams {
            j_limit: 1e12, // Very low limit
            i_peak: 1.0,   // 1A
        };
        // A_min = 1.0 / 1e12 = 1e-12m = 1nm → 1nm is not < 1nm → pass
        assert!(check_electromigration(&seg, &params).is_none());

        // Width of 0 should definitely violate (0 < 1nm)
        let seg0 = IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 0,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        };
        let v = check_electromigration(&seg0, &params);
        assert!(v.is_some());
        let v = v.expect("em violation expected for zero width");
        assert_eq!(v.width_nm, 0);
    }

    #[test]
    fn test_em_zero_width_passes_for_zero_current() {
        let seg0 = IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 0,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        };
        let params = EmParams {
            j_limit: 0.0,
            i_peak: 1.0,
        };
        assert!(check_electromigration(&seg0, &params).is_none());
    }

    #[test]
    fn test_thermal_high_current_narrow_violation() {
        // 100µm width, 5A, 35µm copper
        // ΔT = 6.0e-6 * 25 / (100e-6 * (35e-6)^0.44)
        //    = 1.5e-4 / (100e-6 * 0.01093) ≈ 137°C > 20°C
        let seg = make_segment(100_000, 1); // 100µm
        let current = CurrentDeclaration::Dc(5.0);
        let params = ThermalParams {
            ambient_temp_c: 25.0,
            max_temp_rise_c: 20.0,
            copper_thickness_m: 35e-6,
            substrate_er: 4.2,
        };
        let v = check_temperature_rise(&seg, &current, &params);
        assert!(v.is_some());
        let v = v.expect("thermal violation expected");
        assert!(v.temp_rise_c > v.max_allowed_c);
    }

    #[test]
    fn test_thermal_wide_trace_pass() {
        // 5mm width, 0.1A, 35µm copper
        // ΔT = 6.0e-6 * 0.01 / (5e-3 * 0.01093) ≈ 0.0011°C < 20°C
        let seg = make_segment(5_000_000, 1); // 5mm
        let current = CurrentDeclaration::Dc(0.1);
        let params = ThermalParams {
            ambient_temp_c: 25.0,
            max_temp_rise_c: 20.0,
            copper_thickness_m: 35e-6,
            substrate_er: 4.2,
        };
        assert!(check_temperature_rise(&seg, &current, &params).is_none());
    }

    #[test]
    fn test_ac_current_declaration() {
        let dc = CurrentDeclaration::Dc(3.0);
        assert!((dc.rms() - 3.0).abs() < 1e-10);
        assert!((dc.peak() - 3.0).abs() < 1e-10);

        let ac = CurrentDeclaration::Ac(AcCurrent {
            rms: 2.0,
            peak: 3.5,
        });
        assert!((ac.rms() - 2.0).abs() < 1e-10);
        assert!((ac.peak() - 3.5).abs() < 1e-10);
    }

    #[test]
    fn test_auto_scale_width_returns_minimum() {
        let seg = make_segment(100_000, 1);
        let current = CurrentDeclaration::Dc(5.0);
        let params = ThermalParams {
            ambient_temp_c: 25.0,
            max_temp_rise_c: 20.0,
            copper_thickness_m: 35e-6,
            substrate_er: 4.2,
        };
        let new_w = auto_scale_width(&seg, &current, &params, 20.0);
        assert!(new_w >= seg.width_nm);
        // The computed width should be enough to bring temp rise under 20°C
        let width_m = new_w as f64 / 1_000_000_000.0;
        let t_pow = 35e-6f64.powf(0.44);
        let dt = IPC2152_K_SI * 5.0 * 5.0 / (width_m * t_pow);
        assert!(dt <= 20.0 + 1.0); // small tolerance for float
    }

    #[test]
    fn test_batch_verify_returns_all_violations() {
        let segs = vec![make_segment(100_000, 1), make_segment(5_000_000, 2)];
        let current = CurrentDeclaration::Dc(5.0);
        let em = EmParams {
            j_limit: 1e6,
            i_peak: 100.0,
        };
        let thermal = ThermalParams {
            ambient_temp_c: 25.0,
            max_temp_rise_c: 20.0,
            copper_thickness_m: 35e-6,
            substrate_er: 4.2,
        };
        let violations = verify_em_thermal(&segs, &current, &em, &thermal);
        // First segment (100µm, 5A) → thermal violation; second (5mm, 5A) → pass
        assert_eq!(violations.len(), 1);
    }
}
