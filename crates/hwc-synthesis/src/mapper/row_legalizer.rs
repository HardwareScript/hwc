// crates/hwc-synthesis/src/mapper/row_legalizer.rs

use crate::mapper::placer_loop::PlacedCell;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardCellSiteRow {
    pub y_min_pm: i64,
    pub y_max_pm: i64,
    pub site_width_pm: i64,
    pub is_flipped_y: bool, // Alternates VDD/VSS orientation
}

/// A legalized standard-cell placement record ready for EntityGraph ingestion.
/// `input_automorphism_group` carries the symmetric pin permutation group
/// computed by the NPN canonicalizer, forwarded to `hwc-physics` LVS and
/// `hwc-router` pin swapping without duplicate automorphism solving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalizedCellInstance {
    pub instance_name: CompactString,
    pub cell_type: CompactString,
    pub pos_x_pm: i64,
    pub pos_y_pm: i64,
    pub width_pm: i64,
    pub height_pm: i64,
    pub is_flipped_y: bool,
    /// Permutation automorphism group (S2, S3) derived from 64-bit NPN truth table.
    /// Index vectors represent swappable input pin positions.
    pub input_automorphism_group: Vec<Vec<u8>>,
}

pub struct StandardCellRowLegalizer;

impl StandardCellRowLegalizer {
    /// Generates standard-cell site rows for a rectangular region.
    pub fn generate_rows(
        y_min_pm: i64,
        height_pm: i64,
        row_height_pm: i64,
        site_width_pm: i64,
    ) -> Vec<StandardCellSiteRow> {
        let row_h = row_height_pm.max(1);
        let num_rows = (height_pm / row_h).max(1);
        let mut rows = Vec::with_capacity(num_rows as usize);

        // Snap y_min_pm to the global row grid so VDD/VSS rails align globally
        let start_row_idx = y_min_pm / row_h;
        let aligned_y_min = start_row_idx * row_h;

        for i in 0..num_rows {
            let r_y_min = aligned_y_min + i * row_h;
            let r_y_max = r_y_min + row_h;
            // Alternate orientation based on global row index so power rails abut
            let is_flipped_y = ((start_row_idx + i) % 2) != 0;
            rows.push(StandardCellSiteRow {
                y_min_pm: r_y_min,
                y_max_pm: r_y_max,
                site_width_pm,
                is_flipped_y,
            });
        }

        rows
    }

    /// Snaps continuous quadratic placement coordinates to legal standard-cell sites,
    /// ensuring power rail abutment (VDD/VSS continuous stripes) and zero overlapping.
    pub fn legalize_to_rows(
        raw_instances: &[PlacedCell],
        rows: &[StandardCellSiteRow],
    ) -> Vec<LegalizedCellInstance> {
        if rows.is_empty() || raw_instances.is_empty() {
            return Vec::new();
        }

        // Group cells by nearest row
        let mut row_assignments: Vec<Vec<&PlacedCell>> = vec![Vec::new(); rows.len()];

        for inst in raw_instances {
            let best_row_idx = rows
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| (r.y_min_pm - inst.raw_y_pm).abs())
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            row_assignments[best_row_idx].push(inst);
        }

        let mut legalized = Vec::with_capacity(raw_instances.len());

        for (row_idx, cells) in row_assignments.iter_mut().enumerate() {
            let row = &rows[row_idx];
            // Sort cells in row by X coordinate
            cells.sort_by_key(|c| c.raw_x_pm);

            let mut current_x = 0i64;
            for cell in cells.iter() {
                // Snap X coordinate to site width multiples
                let mut snapped_x = (cell.raw_x_pm / row.site_width_pm) * row.site_width_pm;
                if snapped_x < current_x {
                    snapped_x = current_x;
                }

                legalized.push(LegalizedCellInstance {
                    instance_name: cell.instance_name.clone(),
                    cell_type: cell.cell_type.clone(),
                    pos_x_pm: snapped_x,
                    pos_y_pm: row.y_min_pm,
                    width_pm: cell.width_pm,
                    height_pm: cell.height_pm,
                    is_flipped_y: row.is_flipped_y,
                    input_automorphism_group: cell.symmetries.clone(),
                });

                current_x = snapped_x + cell.width_pm;
            }
        }

        legalized
    }
}
