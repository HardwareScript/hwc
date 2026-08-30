//! Stage 1: Pin Access Analysis (PAA)
//!
//! Pre-computes legal, on-grid access points with enclosure scoring for every
//! standard cell pin and macro interface port, eliminating off-grid starvation.

pub mod scoring;

pub use scoring::{score_access_point, PaaScoringConfig};

use crate::traits::RoutingError;
use crate::types::{AccessPoint, PinAccessMap};
use hwc_engine::geometry::Point3D;
use hwc_engine::EntityGraph;

/// Pin Access Analyzer
#[derive(Debug, Clone)]
pub struct PinAccessAnalyzer {
    config: PaaScoringConfig,
}

impl PinAccessAnalyzer {
    pub fn new(config: PaaScoringConfig) -> Self {
        Self { config }
    }

    /// Evaluates pin access points for all entities in the graph.
    pub fn analyze(
        &self,
        entity_graph: &EntityGraph,
    ) -> Result<PinAccessMap, RoutingError> {
        let mut map = PinAccessMap::new();

        // 1. Process component metadata terminals
        for (comp_idx, comp_meta) in entity_graph.component_metadata.iter().enumerate() {
            let comp_id = comp_idx as u32;

            for term in &comp_meta.terminals {
                let pin_name = term.name.clone();
                let center_x = term.position.x;
                let center_y = term.position.y;
                let center_z = term.position.z;

                let width = 200_000; // 200 nm default pin width
                let height = 200_000;

                let min_x = center_x - width / 2;
                let max_x = center_x + width / 2;
                let min_y = center_y - height / 2;
                let max_y = center_y + height / 2;

                let mut candidates = Vec::new();

                // Center access point candidate
                let center_pt = Point3D::new(center_x, center_y, center_z);
                if let Some(ap) = score_access_point(
                    center_pt,
                    0,
                    true,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    &self.config,
                ) {
                    candidates.push(ap);
                }

                // Grid-aligned offset candidates (e.g. left/right offset if within enclosure)
                let offset_step = self.config.via_landing_diameter_pm / 4;
                for dx in &[-offset_step, offset_step] {
                    let off_pt = Point3D::new(center_x + dx, center_y, center_z);
                    if let Some(ap) = score_access_point(
                        off_pt,
                        0,
                        false,
                        min_x,
                        max_x,
                        min_y,
                        max_y,
                        &self.config,
                    ) {
                        candidates.push(ap);
                    }
                }

                if candidates.is_empty() {
                    // Check if center can serve as legal minimal point
                    let fallback_ap = AccessPoint {
                        point: center_pt,
                        layer_idx: 0,
                        score: 100,
                        is_preferred: true,
                    };
                    candidates.push(fallback_ap);
                }

                candidates.sort_by(|a, b| b.score.cmp(&a.score));
                map.insert(comp_id, pin_name, candidates);
            }
        }

        // 2. Process component pins registered on entity graph
        for (idx, pin) in entity_graph.get_component_pins().iter().enumerate() {
            let pt = Point3D::new(pin.x_nm * 1000, pin.y_nm * 1000, pin.z_nm * 1000);
            let ap = AccessPoint {
                point: pt,
                layer_idx: 0,
                score: 500,
                is_preferred: true,
            };
            let comp_id = idx as u32;
            map.insert(comp_id, pin.pin_name.clone(), vec![ap]);
        }

        Ok(map)
    }
}
