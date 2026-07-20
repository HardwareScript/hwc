//! Ghost (duplicate) segment registry for G-cell boundary handling.

use crate::geometry::BoundingBox;
use crate::geometry_router::spatial_index::IndexedSegment;

/// Tracks ghost (duplicate) segments across adjacent G-cells.
///
/// When a segment is within `max_clearance_nm` of a G-cell boundary,
/// it must be registered in both adjacent cells as a ghost duplicate.
/// The registry identifies which segments in the local list are ghosts
/// (their center lies outside the unexpanded cell bounds).
#[derive(Clone, Debug)]
pub struct GhostRegistry {
    ghost_indices: Vec<usize>,
}

impl GhostRegistry {
    pub fn new() -> Self {
        Self {
            ghost_indices: Vec::new(),
        }
    }

    #[inline]
    pub fn register_ghost(&mut self, local_index: usize) {
        self.ghost_indices.push(local_index);
    }

    #[inline]
    pub fn is_ghost(&self, local_index: usize) -> bool {
        self.ghost_indices.contains(&local_index)
    }

    /// Build a ghost registry from segments and cell bounds.
    ///
    /// A segment is a ghost if its center is outside the unexpanded cell
    /// but was included because it falls within the halo-expanded query region.
    pub fn from_segments(segments: &[IndexedSegment], cell_bounds: &BoundingBox) -> Self {
        let mut registry = Self::new();
        for (i, seg) in segments.iter().enumerate() {
            let center = seg.center();
            if !cell_bounds.contains(center) {
                registry.register_ghost(i);
            }
        }
        registry
    }
}

impl Default for GhostRegistry {
    fn default() -> Self {
        Self::new()
    }
}
