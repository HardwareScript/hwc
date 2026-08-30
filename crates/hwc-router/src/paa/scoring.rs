//! Pin Access Point Scoring and Validation
//!
//! Evaluates on-grid access points for standard-cell pins and macro ports,
//! scoring each candidate by track alignment, enclosure margin, and via landing area.

use crate::types::AccessPoint;
use hwc_engine::geometry::Point3D;

/// Configuration parameters for Pin Access Analysis scoring.
#[derive(Debug, Clone)]
pub struct PaaScoringConfig {
    pub min_enclosure_pm: i64,
    pub via_landing_diameter_pm: i64,
    pub preferred_direction_bonus: u16,
}

impl Default for PaaScoringConfig {
    fn default() -> Self {
        Self {
            min_enclosure_pm: 50_000,          // 50 nm
            via_landing_diameter_pm: 100_000,  // 100 nm
            preferred_direction_bonus: 500,
        }
    }
}

/// Evaluates and scores an access candidate point.
pub fn score_access_point(
    point: Point3D,
    layer_idx: u8,
    is_preferred: bool,
    pin_min_x: i64,
    pin_max_x: i64,
    pin_min_y: i64,
    pin_max_y: i64,
    config: &PaaScoringConfig,
) -> Option<AccessPoint> {
    // 1. Check enclosure bounds in picometers
    let half_via = config.via_landing_diameter_pm / 2;
    let enc_left = (point.x - half_via) - pin_min_x;
    let enc_right = pin_max_x - (point.x + half_via);
    let enc_bottom = (point.y - half_via) - pin_min_y;
    let enc_top = pin_max_y - (point.y + half_via);

    if enc_left < 0 || enc_right < 0 || enc_bottom < 0 || enc_top < 0 {
        // Via extends outside pin boundary
        return None;
    }

    let min_enc = enc_left.min(enc_right).min(enc_bottom).min(enc_top);
    let enclosure_score = ((min_enc / 1000).max(0) as u16).min(1000);

    let mut score = enclosure_score;
    if is_preferred {
        score = score.saturating_add(config.preferred_direction_bonus);
    }

    Some(AccessPoint {
        point,
        layer_idx,
        score,
        is_preferred,
    })
}
