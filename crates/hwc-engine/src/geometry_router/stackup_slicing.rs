//! Stackup Slicing Engine — maps 3D geometry to per-layer 2D footprints.
//!
//! Given a PCB stackup (ordered Z-intervals) and a 3D bounding box,
//! this module slices the box into per-layer 2D polygons for the autorouter.

use std::collections::HashMap;

use crate::geometry::BoundingBox;

/// A single layer in the PCB stackup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackupLayer {
    pub name: String,
    pub z_min_nm: i64,
    pub z_max_nm: i64,
    pub material_id: u8,
    /// v0.1.8: Whether this layer permits routing.
    /// Table-driven constraint — the pathfinder consults this before placing
    /// trace segments. `None` defaults to full routing (backward compatible).
    pub routable: Option<RoutableMode>,
}

/// Whether a stackup layer permits routing (v0.1.8 Physical Synthesis Guardrails).
///
/// Mirrors the parser-side `RoutableMode` but lives in the engine to avoid
/// cross-crate dependencies. The engine uses this enum for O(1) pattern
/// matching in the hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoutableMode {
    /// Full routing permitted (metal layers)
    True,
    /// No routing permitted (substrate, active, oxide)
    False,
    /// Local interconnects only with max length limit
    LocalOnly,
}

/// Manages the ordered stackup of PCB layers.
#[derive(Clone, Debug)]
pub struct StackupManager {
    layers: Vec<StackupLayer>,
    /// Pre-sorted Z-intervals (z_min, z_max) in ascending order.
    z_intervals: Vec<(i64, i64)>,
}

impl StackupManager {
    /// Create a new manager from an unsorted list of layers.
    ///
    /// Layers are sorted by `z_min_nm` ascending.
    pub fn new(mut layers: Vec<StackupLayer>) -> Self {
        layers.sort_by_key(|l| l.z_min_nm);
        let z_intervals: Vec<(i64, i64)> =
            layers.iter().map(|l| (l.z_min_nm, l.z_max_nm)).collect();
        Self {
            layers,
            z_intervals,
        }
    }

    /// Returns sorted Z-intervals `(z_min, z_max)`.
    #[inline]
    pub fn get_ordered_z_intervals(&self) -> &[(i64, i64)] {
        &self.z_intervals
    }

    /// Binary search for the layer containing the given Z coordinate.
    #[inline]
    pub fn find_layer_at_z(&self, z_nm: i64) -> Option<&StackupLayer> {
        // Binary search: find the last layer whose z_min <= z_nm.
        let idx = match self
            .z_intervals
            .binary_search_by(|(z_min, _)| z_min.cmp(&z_nm))
        {
            Ok(i) => i,
            Err(i) => i.wrapping_sub(1), // insertion point minus one
        };
        let layer = self.layers.get(idx)?;
        if z_nm >= layer.z_min_nm && z_nm < layer.z_max_nm {
            Some(layer)
        } else {
            None
        }
    }

    /// Number of layers in the stackup.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Whether the stackup is empty.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Query whether a layer permits routing (v0.1.8 Physical Synthesis Guardrails).
    ///
    /// Returns the `RoutableMode` for the named layer. If the layer is not
    /// found or has no declared routable mode, defaults to `RoutableMode::True`
    /// for backward compatibility with legacy profiles.
    #[inline]
    pub fn is_routable(&self, layer_name: &str) -> RoutableMode {
        self.layers
            .iter()
            .find(|l| l.name == layer_name)
            .and_then(|l| l.routable)
            .unwrap_or(RoutableMode::True)
    }

    /// Check if a layer is non-routable (`routable: false`).
    #[inline]
    pub fn is_non_routable(&self, layer_name: &str) -> bool {
        self.is_routable(layer_name) == RoutableMode::False
    }

    /// Check if a layer is local-only (`routable: local_only`).
    #[inline]
    pub fn is_local_only(&self, layer_name: &str) -> bool {
        self.is_routable(layer_name) == RoutableMode::LocalOnly
    }

    /// Returns (epsilon_r, z_ground_nm) for the given layer.
    pub fn get_stackup_dielectric_context(&self, layer_name: &str) -> Option<(f64, i64)> {
        let _layer = self.layers.iter().find(|l| l.name == layer_name)?;
        Some((3.9, 0))
    }

    /// Returns the routing centerline elevation in nanometers.
    pub fn get_layer_routing_z(&self, layer_name: &str) -> Option<i64> {
        let layer = self.layers.iter().find(|l| l.name == layer_name)?;
        Some((layer.z_min_nm + layer.z_max_nm) / 2)
    }
}

/// 2D polygon produced by slicing a 3D entity onto a single layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlicedPolygon {
    pub layer_id: u8,
    pub z_min_nm: i64,
    pub z_max_nm: i64,
    /// Ordered 2D vertex list in XY plane (nanometers).
    pub polygon_2d: Vec<(i64, i64)>,
}

/// Slice a 3D bounding box into per-layer 2D polygons.
///
/// For each layer whose Z-interval overlaps the box's Z-span, emits a
/// `SlicedPolygon` whose 2D polygon is the XY rectangle of the box.
/// The polygon is 4 vertices: bottom-left → bottom-right → top-right → top-left.
#[inline]
pub fn slice_entity_to_layers(bbox: &BoundingBox, stackup: &StackupManager) -> Vec<SlicedPolygon> {
    let z_lo = bbox.min.z;
    let z_hi = bbox.max.z;
    let mut result = Vec::with_capacity(stackup.len());

    for (z_min, z_max) in &stackup.z_intervals {
        // Skip layers with no Z overlap.
        if *z_max <= z_lo || *z_min >= z_hi {
            continue;
        }
        let clipped_z_min = z_lo.max(*z_min);
        let clipped_z_max = z_hi.min(*z_max);

        // Find the matching layer for the material_id.
        let layer_id = stackup
            .layers
            .iter()
            .find(|l| l.z_min_nm == *z_min && l.z_max_nm == *z_max)
            .map_or(0, |l| l.material_id);

        // Project XY rectangle.
        let polygon_2d = vec![
            (bbox.min.x, bbox.min.y),
            (bbox.max.x, bbox.min.y),
            (bbox.max.x, bbox.max.y),
            (bbox.min.x, bbox.max.y),
        ];

        result.push(SlicedPolygon {
            layer_id,
            z_min_nm: clipped_z_min,
            z_max_nm: clipped_z_max,
            polygon_2d,
        });
    }

    result
}

/// Registry of 2D shapes keyed by layer ID.
#[derive(Clone, Debug, Default)]
pub struct LayerShapeRegistry {
    shapes: HashMap<u8, Vec<SlicedPolygon>>,
}

impl LayerShapeRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (append) sliced polygons into the registry.
    ///
    /// Shapes are appended to existing layer entries; no new layer object
    /// is allocated when the layer already exists.
    pub fn register_shapes(&mut self, shapes: Vec<SlicedPolygon>) {
        for s in shapes {
            self.shapes.entry(s.layer_id).or_default().push(s);
        }
    }

    /// Get the shapes for a given layer.
    #[inline]
    pub fn get_layer_shapes(&self, layer_id: u8) -> &[SlicedPolygon] {
        self.shapes.get(&layer_id).map_or(&[], |v| v.as_slice())
    }

    /// Total number of registered shapes across all layers.
    #[inline]
    pub fn total_shapes(&self) -> usize {
        self.shapes.values().fold(0usize, |acc, v| acc + v.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;

    fn make_stackup() -> StackupManager {
        StackupManager::new(vec![
            StackupLayer {
                name: "M1".into(),
                z_min_nm: 0,
                z_max_nm: 100_000,
                material_id: 0,
                routable: None,
            },
            StackupLayer {
                name: "M2".into(),
                z_min_nm: 100_000,
                z_max_nm: 200_000,
                material_id: 1,
                routable: None,
            },
            StackupLayer {
                name: "M3".into(),
                z_min_nm: 200_000,
                z_max_nm: 300_000,
                material_id: 2,
                routable: None,
            },
        ])
    }

    #[test]
    fn find_layer_at_z_binary_search() {
        let stackup = make_stackup();

        // Exact z_min should find the layer.
        assert_eq!(
            stackup.find_layer_at_z(0).map(|l| l.name.as_str()),
            Some("M1")
        );
        assert_eq!(
            stackup.find_layer_at_z(100_000).map(|l| l.name.as_str()),
            Some("M2")
        );
        assert_eq!(
            stackup.find_layer_at_z(200_000).map(|l| l.name.as_str()),
            Some("M3")
        );

        // Midpoint.
        assert_eq!(
            stackup.find_layer_at_z(150_000).map(|l| l.name.as_str()),
            Some("M2")
        );

        // z_max is exclusive.
        assert_eq!(stackup.find_layer_at_z(300_000), None);

        // Out of range.
        assert_eq!(stackup.find_layer_at_z(-1), None);
        assert_eq!(stackup.find_layer_at_z(400_000), None);
    }

    #[test]
    fn entity_spans_multiple_layers() {
        let stackup = make_stackup();
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 50_000),
            Point3D::new(1_000_000, 1_000_000, 250_000),
        );

        let slices = slice_entity_to_layers(&bbox, &stackup);
        // Overlaps M1 (partial), M2 (full), M3 (partial) → 3 slices.
        assert_eq!(slices.len(), 3);

        // Verify layer ordering.
        assert_eq!(slices[0].layer_id, 0);
        assert_eq!(slices[1].layer_id, 1);
        assert_eq!(slices[2].layer_id, 2);
    }

    #[test]
    fn entity_fully_within_one_layer() {
        let stackup = make_stackup();
        let bbox = BoundingBox::new(
            Point3D::new(100, 200, 120_000),
            Point3D::new(500, 600, 180_000),
        );

        let slices = slice_entity_to_layers(&bbox, &stackup);
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].layer_id, 1);
        assert_eq!(slices[0].z_min_nm, 120_000);
        assert_eq!(slices[0].z_max_nm, 180_000);
        // Verify XY polygon matches bbox.
        assert_eq!(
            slices[0].polygon_2d,
            vec![(100, 200), (500, 200), (500, 600), (100, 600)]
        );
    }

    #[test]
    fn shape_registration_accumulates() {
        let stackup = make_stackup();
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 100_000),
            Point3D::new(1_000, 1_000, 200_000),
        );

        let slices = slice_entity_to_layers(&bbox, &stackup);
        assert_eq!(slices.len(), 1);

        let mut registry = LayerShapeRegistry::new();
        registry.register_shapes(slices.clone());
        assert_eq!(registry.total_shapes(), 1);
        assert_eq!(registry.get_layer_shapes(1).len(), 1);

        // Register more shapes for same layer.
        registry.register_shapes(slices);
        assert_eq!(registry.total_shapes(), 2);
        assert_eq!(registry.get_layer_shapes(1).len(), 2);

        // Empty layer returns empty slice.
        assert!(registry.get_layer_shapes(99).is_empty());
    }

    #[test]
    fn ordered_z_intervals() {
        let stackup = make_stackup();
        let intervals = stackup.get_ordered_z_intervals();
        assert_eq!(
            intervals,
            &[(0, 100_000), (100_000, 200_000), (200_000, 300_000)]
        );
    }

    #[test]
    fn entity_no_z_overlap() {
        let stackup = make_stackup();
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 500_000),
            Point3D::new(1_000, 1_000, 600_000),
        );
        let slices = slice_entity_to_layers(&bbox, &stackup);
        assert!(slices.is_empty());
    }
}
