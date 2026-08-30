//! Freeze-Silicon ECO Boolean Patch & GA-Filler Allocation
//!
//! Validates base silicon immutability (Layers 1-20) and maps Boolean ECO patches
//! to uncommitted Gate-Array (GA) filler cells pre-fabricated on the wafer.

use crate::traits::RoutingError;
use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use rustc_hash::FxHashSet;

#[derive(Debug, Clone)]
pub struct GaFillerCell {
    pub id: u32,
    pub name: CompactString,
    pub location: Point3D,
    pub is_committed: bool,
}

pub struct EcoPatchManager {
    pub locked_layers: FxHashSet<CompactString>,
}

impl Default for EcoPatchManager {
    fn default() -> Self {
        let mut locked = FxHashSet::default();
        // Base silicon layers 1 to 20
        for name in &[
            "diff", "poly", "licon", "psdm", "nsdm", "nwell", "pwell",
            "tap", "rpm", "npc", "dnwell", "gate", "active"
        ] {
            locked.insert(CompactString::new(*name));
        }
        Self { locked_layers: locked }
    }
}

impl EcoPatchManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asserts that no base silicon layers have been mutated.
    pub fn verify_base_silicon_immutability(
        &self,
        mutated_layers: &[CompactString],
    ) -> Result<(), RoutingError> {
        for layer in mutated_layers {
            if self.locked_layers.contains(layer) {
                return Err(RoutingError::FreezeSiliconViolation {
                    layer: layer.to_string(),
                    mutation_count: 1,
                });
            }
        }
        Ok(())
    }

    /// Finds nearest available uncommitted GA filler cell to a target coordinate.
    pub fn find_nearest_spare_filler<'a>(
        &self,
        target: Point3D,
        spares: &'a mut [GaFillerCell],
    ) -> Option<&'a mut GaFillerCell> {
        spares
            .iter_mut()
            .filter(|f| !f.is_committed)
            .min_by_key(|f| {
                let dx = f.location.x - target.x;
                let dy = f.location.y - target.y;
                dx * dx + dy * dy
            })
    }
}
