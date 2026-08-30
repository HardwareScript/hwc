// crates/hwc-synthesis/src/types.rs

use crate::mapper::row_legalizer::LegalizedCellInstance;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// High-level synthesis options and constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisOptions {
    pub target_freq_mhz: f32,
    pub enable_fraig: bool,
    pub enable_word_rewrite: bool,
    pub enable_cec: bool,
    /// Placement boundary: (x_min_pm, y_min_pm, width_pm, height_pm)
    pub region_boundary: (i64, i64, i64, i64),
}

impl Default for SynthesisOptions {
    fn default() -> Self {
        Self {
            target_freq_mhz: 100.0,
            enable_fraig: true,
            enable_word_rewrite: true,
            enable_cec: true,
            region_boundary: (0, 0, 50_000_000, 30_000_000), // 50um x 30um
        }
    }
}

/// Comprehensive synthesis result ready for EntityGraph ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub top_module_name: CompactString,
    pub legalized_cells: Vec<LegalizedCellInstance>,
    pub gate_count: usize,
    pub total_area_pm2: i128,
    pub max_delay_ps: f32,
    pub cec_verified: bool,
}
