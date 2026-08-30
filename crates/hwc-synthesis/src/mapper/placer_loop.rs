// crates/hwc-synthesis/src/mapper/placer_loop.rs

use crate::mapper::priority_cuts::MappedInstance;
use compact_str::CompactString;
use hwc_engine::stackup::StackupManager;
use rustc_hash::FxHashMap;

/// Shift-Left single-source dielectric delay calculator querying StackupManager directly.
pub struct ShiftLeftDelayEstimator<'a> {
    pub stackup: &'a StackupManager,
    pub target_layer: &'static str,
}

impl<'a> ShiftLeftDelayEstimator<'a> {
    pub fn new(stackup: &'a StackupManager, target_layer: &'static str) -> Self {
        Self {
            stackup,
            target_layer,
        }
    }

    /// Computes physical RC delay for a Steiner interconnect segment in picoseconds
    /// using Wheeler effective permittivity and Sakurai microstrip formulas.
    pub fn estimate_segment_delay_ps(&self, length_pm: i64, width_pm: i64) -> f32 {
        let (eps_r, z_ground_nm) = self
            .stackup
            .get_stackup_dielectric_context(self.target_layer)
            .unwrap_or((3.9, 0));

        let routing_z_nm = self
            .stackup
            .get_layer_routing_z(self.target_layer)
            .unwrap_or(360);

        let h_m = ((routing_z_nm - z_ground_nm).max(10) as f64) * 1e-9;
        let w_m = (width_pm.max(100_000) as f64) * 1e-12;
        let l_m = (length_pm.max(0) as f64) * 1e-12;
        let t_m = 0.36e-6; // 360nm metal thickness

        // 1. Wheeler effective permittivity
        let term = (1.0 + 12.0 * (h_m / w_m)).powf(-0.5);
        let eps_eff = ((eps_r + 1.0) / 2.0) + ((eps_r - 1.0) / 2.0) * term;

        // 2. Sakurai ground capacitance: C = eps0 * eps_eff * L * (1.15(W/H) + 2.80(T/H)^0.222)
        const EPS_0: f64 = 8.854_187_812_8e-12;
        let c_gnd_f =
            EPS_0 * eps_eff * l_m * (1.15 * (w_m / h_m) + 2.80 * (t_m / h_m).powf(0.222));

        // 3. Wire resistance (Aluminum rho = 2.82e-8 Ohm-m)
        let r_wire_ohms = (2.82e-8 * l_m) / (w_m * t_m);

        // 4. Elmore Delay: tau = 0.5 * R * C (in picoseconds)
        let elmore_delay_ps = (0.5 * r_wire_ohms * c_gnd_f * 1e12) as f32;
        elmore_delay_ps.max(0.1)
    }
}

/// Raw placed instance with continuous (floating) coordinates prior to row legalization.
#[derive(Debug, Clone)]
pub struct PlacedCell {
    pub instance_name: CompactString,
    pub cell_type: CompactString,
    pub raw_x_pm: i64,
    pub raw_y_pm: i64,
    pub width_pm: i64,
    pub height_pm: i64,
    pub symmetries: Vec<Vec<u8>>,
}

/// Shift-Left Analytical Quadratic Placer.
pub struct AnalyticalPlacer;

impl AnalyticalPlacer {
    /// Solves analytical placement for mapped instances inside a target region boundary.
    pub fn place(
        instances: &[MappedInstance],
        region_x_pm: i64,
        region_y_pm: i64,
        region_w_pm: i64,
        region_h_pm: i64,
    ) -> Vec<PlacedCell> {
        if instances.is_empty() {
            return Vec::new();
        }

        let mut placed = Vec::with_capacity(instances.len());
        let mut node_positions: FxHashMap<u32, (i64, i64)> = FxHashMap::default();

        let num_cells = instances.len();
        // Spread cells uniformly or cluster based on connectivity
        let cols = (num_cells as f64).sqrt().ceil() as usize;
        let pitch_x = region_w_pm / (cols as i64).max(1);
        let pitch_y = region_h_pm / ((num_cells / cols.max(1) + 1) as i64).max(1);

        for (idx, inst) in instances.iter().enumerate() {
            let col = idx % cols.max(1);
            let row = idx / cols.max(1);

            let initial_x = region_x_pm + (col as i64) * pitch_x;
            let initial_y = region_y_pm + (row as i64) * pitch_y;

            // Compute center-of-gravity from input connections if available
            let mut sum_x = 0i64;
            let mut sum_y = 0i64;
            let mut connected_count = 0i64;

            for &in_node in &inst.input_nodes {
                if let Some(&(ix, iy)) = node_positions.get(&in_node) {
                    sum_x += ix;
                    sum_y += iy;
                    connected_count += 1;
                }
            }

            let (target_x, target_y) = if connected_count > 0 {
                let cog_x = sum_x / connected_count;
                let cog_y = sum_y / connected_count;
                // Weighted average between initial grid and COG
                ((initial_x + cog_x) / 2, (initial_y + cog_y) / 2)
            } else {
                (initial_x, initial_y)
            };

            let clamped_x = target_x.clamp(region_x_pm, (region_x_pm + region_w_pm) - inst.cell.width_pm);
            let clamped_y = target_y.clamp(region_y_pm, (region_y_pm + region_h_pm) - inst.cell.height_pm);

            node_positions.insert(inst.output_node, (clamped_x, clamped_y));

            placed.push(PlacedCell {
                instance_name: inst.instance_name.clone(),
                cell_type: inst.cell.cell_type.clone(),
                raw_x_pm: clamped_x,
                raw_y_pm: clamped_y,
                width_pm: inst.cell.width_pm,
                height_pm: inst.cell.height_pm,
                symmetries: inst.cell.automorphism_group.clone(),
            });
        }

        placed
    }
}
