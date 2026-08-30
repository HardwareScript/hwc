// crates/hwc-synthesis/src/liberty/cell.rs

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Standard-cell definition parsed from Liberty (.lib) format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardCell {
    pub name: CompactString,
    pub cell_type: CompactString,
    pub area_pm2: i128,
    pub width_pm: i64,
    pub height_pm: i64,
    pub delay_ps: f32,
    pub input_pins: Vec<CompactString>,
    pub output_pins: Vec<CompactString>,
    pub truth_table: u64,
    /// Permutation automorphism group (S2, S3) computed from truth table symmetries.
    /// Lists swappable input pin index vectors.
    pub automorphism_group: Vec<Vec<u8>>,
    pub is_sequential: bool,
}

impl StandardCell {
    pub fn new(
        name: &str,
        cell_type: &str,
        width_pm: i64,
        height_pm: i64,
        delay_ps: f32,
        input_pins: &[&str],
        output_pins: &[&str],
        truth_table: u64,
        automorphism_group: Vec<Vec<u8>>,
        is_sequential: bool,
    ) -> Self {
        let area_pm2 = i128::from(width_pm) * i128::from(height_pm);
        Self {
            name: CompactString::new(name),
            cell_type: CompactString::new(cell_type),
            area_pm2,
            width_pm,
            height_pm,
            delay_ps,
            input_pins: input_pins.iter().map(|&p| CompactString::new(p)).collect(),
            output_pins: output_pins.iter().map(|&p| CompactString::new(p)).collect(),
            truth_table,
            automorphism_group,
            is_sequential,
        }
    }
}
