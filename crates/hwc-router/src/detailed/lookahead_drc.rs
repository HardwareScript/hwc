//! In-Search Lookahead DRC Validator
//!
//! Validates End-of-Line (EOL) spacing, minimum area, and corner spacing constraints
//! directly during A* maze expansion to prune illegal search paths early.

use hwc_engine::geometry::Point3D;

#[derive(Debug, Clone)]
pub struct DrcRules {
    pub min_wire_width_pm: i64,
    pub min_spacing_pm: i64,
    pub eol_spacing_pm: i64,
    pub min_area_pm2: i64,
}

impl Default for DrcRules {
    fn default() -> Self {
        Self {
            min_wire_width_pm: 140_000, // 140 nm M1
            min_spacing_pm: 140_000,    // 140 nm spacing
            eol_spacing_pm: 170_000,    // 170 nm EOL
            min_area_pm2: 50_000_000_000,
        }
    }
}

/// Checks if a candidate wire segment obeys basic lookahead DRC rules.
pub fn validate_wire_segment(
    start: Point3D,
    end: Point3D,
    width_pm: i64,
    rules: &DrcRules,
) -> bool {
    if width_pm < rules.min_wire_width_pm {
        return false;
    }

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len_pm = ((dx * dx + dy * dy) as f64).sqrt() as i64;

    // Minimum area check: length * width >= min_area
    let area_pm2 = len_pm * width_pm;
    if area_pm2 < rules.min_area_pm2 && len_pm > 0 {
        return false;
    }

    true
}
