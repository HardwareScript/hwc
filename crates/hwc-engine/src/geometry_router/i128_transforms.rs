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
/// Results are clamped to i64 range before returning.
#[inline]
pub fn transform_point_i128(
    x: i64,
    y: i64,
    cos_nm: i128,
    sin_nm: i128,
    tx_nm: i128,
    ty_nm: i128,
) -> (i64, i64) {
    let x_i = x as i128;
    let y_i = y as i128;

    let rx = (x_i * cos_nm - y_i * sin_nm) / SCALE_FACTOR + tx_nm;
    let ry = (x_i * sin_nm + y_i * cos_nm) / SCALE_FACTOR + ty_nm;

    (clamp_to_i64(rx), clamp_to_i64(ry))
}

/// Transform a bounding box by transforming all 4 corners and computing
/// the new axis-aligned bounding box.
///
/// Returns `(new_min_x, new_min_y, new_max_x, new_max_y)`.
#[inline]
pub fn transform_bbox_i128(
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
    cos_nm: i128,
    sin_nm: i128,
    tx_nm: i128,
    ty_nm: i128,
) -> (i64, i64, i64, i64) {
    let corners = [
        transform_point_i128(min_x, min_y, cos_nm, sin_nm, tx_nm, ty_nm),
        transform_point_i128(max_x, min_y, cos_nm, sin_nm, tx_nm, ty_nm),
        transform_point_i128(min_x, max_y, cos_nm, sin_nm, tx_nm, ty_nm),
        transform_point_i128(max_x, max_y, cos_nm, sin_nm, tx_nm, ty_nm),
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

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: i64 = 1; // 1 nm tolerance for integer math

    #[test]
    fn transform_point_0deg_identity() {
        let (cos, sin) = trig_values_i128(0);
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 0, 0);
        assert_eq!(rx, 1000);
        assert_eq!(ry, 2000);
    }

    #[test]
    fn transform_point_0deg_with_translation() {
        let (cos, sin) = trig_values_i128(0);
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 500, 300);
        assert_eq!(rx, 1500);
        assert_eq!(ry, 2300);
    }

    #[test]
    fn transform_point_90deg() {
        let (cos, sin) = trig_values_i128(90);
        // 90°: x' = -y, y' = x
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 0, 0);
        assert_eq!(rx, -2000);
        assert_eq!(ry, 1000);
    }

    #[test]
    fn transform_point_90deg_with_translation() {
        let (cos, sin) = trig_values_i128(90);
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 5000, 3000);
        assert_eq!(rx, 3000); // -2000 + 5000
        assert_eq!(ry, 4000); // 1000 + 3000
    }

    #[test]
    fn transform_point_45deg() {
        let (cos, sin) = trig_values_i128(45);
        // 45°: x' = (x - y) * cos(45), y' = (x + y) * cos(45)
        let (rx, ry) = transform_point_i128(1000, 0, cos, sin, 0, 0);
        // x' = 1000 * 707106781 / 1e9 ≈ 707
        assert!((rx - 707).abs() <= TOLERANCE);
        assert!((ry - 707).abs() <= TOLERANCE);
    }

    #[test]
    fn transform_point_180deg() {
        let (cos, sin) = trig_values_i128(180);
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 0, 0);
        assert_eq!(rx, -1000);
        assert_eq!(ry, -2000);
    }

    #[test]
    fn transform_point_270deg() {
        let (cos, sin) = trig_values_i128(270);
        // 270°: x' = y, y' = -x
        let (rx, ry) = transform_point_i128(1000, 2000, cos, sin, 0, 0);
        assert_eq!(rx, 2000);
        assert_eq!(ry, -1000);
    }

    #[test]
    fn transform_bbox_preserves_area_90deg() {
        let (cos, sin) = trig_values_i128(90);
        let (nx0, ny0, nx1, ny1) = transform_bbox_i128(0, 0, 1000, 500, cos, sin, 0, 0);
        // 90° rotation: (0,0)->(0,0), (1000,0)->(0,1000), (0,500)->(-500,0), (1000,500)->(-500,1000)
        // AABB: min=(-500,0), max=(0,1000)
        assert_eq!(nx0, -500);
        assert_eq!(ny0, 0);
        assert_eq!(nx1, 0);
        assert_eq!(ny1, 1000);
        // Area preserved: 1000*500 = 500*1000
        let orig_area = (1000 - 0) * (500 - 0);
        let new_area = (nx1 - nx0) * (ny1 - ny0);
        assert_eq!(orig_area, new_area);
    }

    #[test]
    fn transform_bbox_preserves_area_0deg() {
        let (cos, sin) = trig_values_i128(0);
        let (nx0, ny0, nx1, ny1) = transform_bbox_i128(100, 200, 600, 800, cos, sin, 0, 0);
        assert_eq!(nx0, 100);
        assert_eq!(ny0, 200);
        assert_eq!(nx1, 600);
        assert_eq!(ny1, 800);
    }

    #[test]
    fn overflow_safety_large_coordinates() {
        // 200mm PCB: coordinates up to 2e8 nm
        // Intermediate product: 2e8 * 1e9 = 2e17, fits in i128
        let (cos, sin) = trig_values_i128(45);
        assert!(verify_no_overflow(200_000_000, 200_000_000, cos, sin));
        assert!(verify_no_overflow(-200_000_000, -200_000_000, cos, sin));
    }

    #[test]
    fn overflow_safety_extreme_coordinates() {
        // Even extreme coordinates (30m board) are safe
        let (cos, sin) = trig_values_i128(45);
        assert!(verify_no_overflow(30_000_000_000, 30_000_000_000, cos, sin));
    }

    #[test]
    fn trig_standard_angles() {
        let (c0, s0) = trig_values_i128(0);
        assert_eq!(c0, 1_000_000_000);
        assert_eq!(s0, 0);

        let (c45, s45) = trig_values_i128(45);
        assert_eq!(c45, 707_106_781);
        assert_eq!(s45, 707_106_781);

        let (c90, s90) = trig_values_i128(90);
        assert_eq!(c90, 0);
        assert_eq!(s90, 1_000_000_000);

        let (c180, s180) = trig_values_i128(180);
        assert_eq!(c180, -1_000_000_000);
        assert_eq!(s180, 0);

        let (c270, s270) = trig_values_i128(270);
        assert_eq!(c270, 0);
        assert_eq!(s270, -1_000_000_000);
    }

    #[test]
    fn trig_negative_angle() {
        let (c, s) = trig_values_i128(-90);
        // -90 mod 360 = 270
        assert_eq!(c, 0);
        assert_eq!(s, -1_000_000_000);
    }

    #[test]
    fn trig_over_360() {
        let (c, s) = trig_values_i128(405);
        // 405 mod 360 = 45
        assert_eq!(c, 707_106_781);
        assert_eq!(s, 707_106_781);
    }

    #[test]
    fn transform_point_large_values() {
        let (cos, sin) = trig_values_i128(0);
        let big = 100_000_000i64; // 100mm
        let (rx, ry) = transform_point_i128(big, big, cos, sin, 0, 0);
        assert_eq!(rx, big);
        assert_eq!(ry, big);
    }

    #[test]
    fn transform_bbox_45deg_square() {
        let (cos, sin) = trig_values_i128(45);
        // 1000x1000 square rotated 45°: AABB becomes ~1414x1414
        let (nx0, ny0, nx1, ny1) = transform_bbox_i128(0, 0, 1000, 1000, cos, sin, 0, 0);
        let width = nx1 - nx0;
        let height = ny1 - ny0;
        // Should be approximately 1000 * sqrt(2) ≈ 1414
        assert!((width - 1414).abs() <= 2);
        assert!((height - 1414).abs() <= 2);
    }
}
