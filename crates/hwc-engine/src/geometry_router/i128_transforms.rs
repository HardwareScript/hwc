//! Fixed-point i128 coordinate transforms for PCB autorouter.
//!
//! All coordinates use i64 nanometers. Trigonometric values are scaled by
//! `SCALE_FACTOR = 10^9`. All intermediate arithmetic uses i128 to prevent
//! overflow on 200mm PCBs where coordinate products reach ~1.414e20.

/// Scale factor for fixed-point trigonometric values: 10^9.
pub const SCALE_FACTOR: i128 = 1_000_000_000;

/// sin(45°) × 10^9
pub const SIN_45_NM: i128 = 707_106_781;

/// cos(1°) × 10^9 (approximate)
pub const COS_1_NM: i128 = 999_999_998;

/// Maximum safe coordinate magnitude (200mm = 200_000_000 nm).
/// Intermediate: 2e8 * 1e9 = 2e17, well within i128 range.
pub const MAX_SAFE_COORD_NM: i64 = 200_000_000;

/// Standard-angle trigonometric lookup table entry.
struct TrigEntry {
    angle: i64,
    cos_nm: i128,
    sin_nm: i128,
}

const TRIG_TABLE: &[TrigEntry] = &[
    TrigEntry {
        angle: 0,
        cos_nm: 1_000_000_000,
        sin_nm: 0,
    },
    TrigEntry {
        angle: 45,
        cos_nm: 707_106_781,
        sin_nm: 707_106_781,
    },
    TrigEntry {
        angle: 90,
        cos_nm: 0,
        sin_nm: 1_000_000_000,
    },
    TrigEntry {
        angle: 135,
        cos_nm: -707_106_781,
        sin_nm: 707_106_781,
    },
    TrigEntry {
        angle: 180,
        cos_nm: -1_000_000_000,
        sin_nm: 0,
    },
    TrigEntry {
        angle: 225,
        cos_nm: -707_106_781,
        sin_nm: -707_106_781,
    },
    TrigEntry {
        angle: 270,
        cos_nm: 0,
        sin_nm: -1_000_000_000,
    },
    TrigEntry {
        angle: 315,
        cos_nm: 707_106_781,
        sin_nm: -707_106_781,
    },
];

/// Return (cos_nm, sin_nm) for the given angle in degrees.
///
/// Standard angles (0, 45, 90, 135, 180, 225, 270, 315) return exact
/// lookup values. Other angles use a first-order i128 approximation:
/// `cos(n) ≈ COS_1_NM^n` and `sin(n) ≈ n × SIN_45_NM / 45`.
#[inline]
pub fn trig_values_i128(angle_deg: i64) -> (i128, i128) {
    let normalized = angle_deg.rem_euclid(360);

    for entry in TRIG_TABLE {
        if entry.angle == normalized {
            return (entry.cos_nm, entry.sin_nm);
        }
    }

    // First-order linear approximation for non-standard angles.
    // sin(θ) ≈ θ_rad, cos(θ) ≈ 1 - θ²/2
    // Using fixed-point: sin_nm ≈ angle_deg * SIN_45_NM / 45
    // cos_nm ≈ SCALE_FACTOR - angle_deg² * SIN_45_NM / (45 * SCALE_FACTOR)
    let sin_nm = (normalized as i128) * SIN_45_NM / 45;

    let angle_sq = (normalized as i128) * (normalized as i128);
    let cos_nm = SCALE_FACTOR - angle_sq * SIN_45_NM / (45 * SCALE_FACTOR);

    (cos_nm, sin_nm)
}

/// Transform a single point using i128 intermediate arithmetic.
///
/// Formula (right-handed rotation + translation):
/// ```text
/// x' = (x * cos_nm - y * sin_nm) / SCALE_FACTOR + tx_nm
/// y' = (x * sin_nm + y * cos_nm) / SCALE_FACTOR + ty_nm
/// ```
///
/// Intermediate products fit in i128: max |coord| = 2e8, max |trig| = 1e9,
/// product = 2e17, well under i128::MAX (~1.7e38).
///
/// An affine 2D transform in fixed-point i128 form.
///
/// `x' = (x * cos_nm - y * sin_nm) / SCALE_FACTOR + tx_nm`
/// `y' = (x * sin_nm + y * cos_nm) / SCALE_FACTOR + ty_nm`
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub cos_nm: i128,
    pub sin_nm: i128,
    pub tx_nm: i128,
    pub ty_nm: i128,
}

impl Transform {
    pub fn new(cos_nm: i128, sin_nm: i128, tx_nm: i128, ty_nm: i128) -> Self {
        Self {
            cos_nm,
            sin_nm,
            tx_nm,
            ty_nm,
        }
    }
}

/// Transform a single point by the given transform.
#[inline]
pub fn transform_point(x: i64, y: i64, t: &Transform) -> (i64, i64) {
    let x_i = x as i128;
    let y_i = y as i128;

    let rx = (x_i * t.cos_nm - y_i * t.sin_nm) / SCALE_FACTOR + t.tx_nm;
    let ry = (x_i * t.sin_nm + y_i * t.cos_nm) / SCALE_FACTOR + t.ty_nm;

    (clamp_to_i64(rx), clamp_to_i64(ry))
}

/// Transform a bounding box by transforming all 4 corners and computing
/// the new axis-aligned bounding box.
///
/// Returns `(new_min_x, new_min_y, new_max_x, new_max_y)`.
#[inline]
pub fn transform_bbox(
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
    t: &Transform,
) -> (i64, i64, i64, i64) {
    let corners = [
        transform_point(min_x, min_y, t),
        transform_point(max_x, min_y, t),
        transform_point(min_x, max_y, t),
        transform_point(max_x, max_y, t),
    ];

    let mut out_min_x = i64::MAX;
    let mut out_min_y = i64::MAX;
    let mut out_max_x = i64::MIN;
    let mut out_max_y = i64::MIN;

    for &(cx, cy) in &corners {
        if cx < out_min_x {
            out_min_x = cx;
        }
        if cy < out_min_y {
            out_min_y = cy;
        }
        if cx > out_max_x {
            out_max_x = cx;
        }
        if cy > out_max_y {
            out_max_y = cy;
        }
    }

    (out_min_x, out_min_y, out_max_x, out_max_y)
}

/// Verify that intermediate products fit in i128 for the given inputs.
///
/// Returns `true` if the transform is safe (no overflow), `false` otherwise.
/// Maximum safe coordinate magnitude: ~3.0e19 / max(|cos|, |sin|).
/// With trig values bounded by 10^9, safe range is ±3.0e10 nm (30 meters).
#[inline]
pub fn verify_no_overflow(x: i64, y: i128, cos_nm: i128, sin_nm: i128) -> bool {
    let x_i = x as i128;
    let y_i = y;

    let prod_x_cos = x_i.checked_mul(cos_nm);
    let prod_y_sin = y_i.checked_mul(sin_nm);
    let prod_x_sin = x_i.checked_mul(sin_nm);
    let prod_y_cos = y_i.checked_mul(cos_nm);

    prod_x_cos.is_some() && prod_y_sin.is_some() && prod_x_sin.is_some() && prod_y_cos.is_some()
}

/// Clamp an i128 value to the i64 range.
#[inline]
fn clamp_to_i64(val: i128) -> i64 {
    if val > i64::MAX as i128 {
        i64::MAX
    } else if val < i64::MIN as i128 {
        i64::MIN
    } else {
        val as i64
    }
}
