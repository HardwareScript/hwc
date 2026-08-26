//! HardwareScript v0.3.0 Closed-Loop Comptime Escape Routing & Channel Envelopes
//!
//! Provides boundary keepout contracts for procedural generators (e.g. BGA, QFN, Pin arrays)
//! to prevent inter-component escape shorts in shared channels prior to global routing.

use serde::{Deserialize, Serialize};

/// High-level spatial allocation pass contract for component pin escapes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeEnvelope {
    pub max_reach_north_nm: i64,
    pub max_reach_east_nm: i64,
    pub max_reach_south_nm: i64,
    pub max_reach_west_nm: i64,
    pub allowed_layers: Vec<String>,
}

impl EscapeEnvelope {
    pub fn new(
        max_reach_north_nm: i64,
        max_reach_east_nm: i64,
        max_reach_south_nm: i64,
        max_reach_west_nm: i64,
        allowed_layers: Vec<String>,
    ) -> Self {
        Self {
            max_reach_north_nm,
            max_reach_east_nm,
            max_reach_south_nm,
            max_reach_west_nm,
            allowed_layers,
        }
    }

    /// Creates a uniform reach envelope in all directions
    pub fn uniform(reach_nm: i64, allowed_layers: Vec<String>) -> Self {
        Self {
            max_reach_north_nm: reach_nm,
            max_reach_east_nm: reach_nm,
            max_reach_south_nm: reach_nm,
            max_reach_west_nm: reach_nm,
            allowed_layers,
        }
    }

    /// Clamp a dogbone / fan-out offset vector (in nanometers) within this keepout envelope
    pub fn clamp_offset(&self, offset_x_nm: i64, offset_y_nm: i64) -> (i64, i64) {
        let clamped_x = offset_x_nm.clamp(-self.max_reach_west_nm, self.max_reach_east_nm);
        let clamped_y = offset_y_nm.clamp(-self.max_reach_south_nm, self.max_reach_north_nm);
        (clamped_x, clamped_y)
    }

    /// Calculate inter-component corridor clearance envelope between two adjacent bounding boxes
    pub fn calculate_channel_envelope(
        component_a_box: (i64, i64, i64, i64), // (min_x, min_y, max_x, max_y)
        component_b_box: (i64, i64, i64, i64),
        buffer_nm: i64,
        allowed_layers: Vec<String>,
    ) -> (Self, Self) {
        let (a_min_x, a_min_y, a_max_x, a_max_y) = component_a_box;
        let (b_min_x, b_min_y, b_max_x, b_max_y) = component_b_box;

        // Horizontal channel
        let dx = if a_max_x < b_min_x {
            (b_min_x - a_max_x - buffer_nm).max(0) / 2
        } else if b_max_x < a_min_x {
            (a_min_x - b_max_x - buffer_nm).max(0) / 2
        } else {
            0
        };

        // Vertical channel
        let dy = if a_max_y < b_min_y {
            (b_min_y - a_max_y - buffer_nm).max(0) / 2
        } else if b_max_y < a_min_y {
            (a_min_y - b_max_y - buffer_nm).max(0) / 2
        } else {
            0
        };

        let env_a = Self::new(dy, dx, dy, dx, allowed_layers.clone());
        let env_b = Self::new(dy, dx, dy, dx, allowed_layers);

        (env_a, env_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_envelope_clamping() {
        let env = EscapeEnvelope::new(400, 500, 300, 600, vec!["M1".to_string(), "M2".to_string()]);
        let (cx, cy) = env.clamp_offset(1000, 1000);
        assert_eq!(cx, 500);
        assert_eq!(cy, 400);

        let (cx2, cy2) = env.clamp_offset(-1000, -1000);
        assert_eq!(cx2, -600);
        assert_eq!(cy2, -300);
    }

    #[test]
    fn test_channel_envelope_partition() {
        // Component A from (0, 0) to (1000, 1000)
        // Component B from (2000, 0) to (3000, 1000)
        // Free gap is 1000nm. With 200nm buffer, available corridor is 800nm (400nm each)
        let box_a = (0, 0, 1000, 1000);
        let box_b = (2000, 0, 3000, 1000);
        let (env_a, env_b) = EscapeEnvelope::calculate_channel_envelope(
            box_a,
            box_b,
            200,
            vec!["M1".to_string()],
        );

        assert_eq!(env_a.max_reach_east_nm, 400);
        assert_eq!(env_b.max_reach_west_nm, 400);
    }
}
