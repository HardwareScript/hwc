use crate::geometry::{BoundingBox, Point3D, TraceSegment};
use rstar::{AABB, PointDistance, RTreeObject};

use crate::connectivity::SubstrateLayerMetadata;
use crate::RouteSegmentMetadata;

/// Entity identification for spatial indexing (v0.1.8)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialEntitySource {
    /// A SubstrateLayer (Pour, Contact, etc.)
    SubstrateLayer { index: usize },
    /// A routed trace segment
    RouteSegment { net_idx: usize, seg_idx: usize },
    /// A component instance from the scene graph
    ComponentInstance { instance_id: usize },
}

/// A trace segment indexed in the R*-tree with physical volume awareness.
#[derive(Clone, Debug)]
pub struct IndexedSegment {
    pub source: SpatialEntitySource,
    pub segment_id: usize, // Legacy ID for DRC/Legalizer (v0.1.8)
    pub net_id: usize,
    pub width_nm: i64,
    pub thickness_nm: i64,
    pub start: Point3D,
    pub end: Point3D,
    pub layer: i64,
}

impl PartialEq for IndexedSegment {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.segment_id == other.segment_id
    }
}

impl IndexedSegment {
    pub fn new(
        source: SpatialEntitySource,
        segment_id: usize,
        net_id: usize,
        segment: &TraceSegment,
        layer: i64,
        thickness_nm: i64,
    ) -> Self {
        Self {
            source,
            segment_id,
            net_id,
            width_nm: segment.width_nm,
            thickness_nm,
            start: segment.start,
            end: segment.end,
            layer,
        }
    }

    pub fn center(&self) -> Point3D {
        Point3D::new(
            (self.start.x + self.end.x) / 2,
            (self.start.y + self.end.y) / 2,
            (self.start.z + self.end.z) / 2,
        )
    }
}

impl RTreeObject for IndexedSegment {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let half_width = (self.width_nm as f64) / 2_000_000.0;
        let min_x = (self.start.x.min(self.end.x) as f64) / 1_000_000.0 - half_width;
        let max_x = (self.start.x.max(self.end.x) as f64) / 1_000_000.0 + half_width;
        let min_y = (self.start.y.min(self.end.y) as f64) / 1_000_000.0 - half_width;
        let max_y = (self.start.y.max(self.end.y) as f64) / 1_000_000.0 + half_width;
        AABB::from_corners([min_x, min_y], [max_x, max_y])
    }
}

impl PointDistance for IndexedSegment {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let center = self.center();
        let dx = point[0] - (center.x as f64 / 1_000_000.0);
        let dy = point[1] - (center.y as f64 / 1_000_000.0);
        dx * dx + dy * dy
    }
}

pub struct DynamicSpatialIndex {
    tree: rstar::RTree<IndexedSegment>,
}

impl DynamicSpatialIndex {
    pub fn new() -> Self {
        Self { tree: rstar::RTree::new() }
    }

    pub fn insert(&mut self, segment: IndexedSegment) {
        self.tree.insert(segment);
    }

    /// Iterate over all entities in the spatial index (v0.1.8).
    pub fn iter(&self) -> impl Iterator<Item = &IndexedSegment> {
        self.tree.iter()
    }

    /// Check if the spatial index is empty (v0.1.8).
    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }

    /// Get the number of entities in the spatial index (v0.1.8).
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    /// Explicitly remove an entity from the spatial index without using "dummy" objects (v0.1.8).
    pub fn remove_by_source(&mut self, source: SpatialEntitySource) -> bool {
        // rstar's RTree doesn't support removal by predicate easily. 
        // We find the element first, then remove it.
        let mut to_remove = None;
        for item in self.tree.iter() {
            if item.source == source {
                to_remove = Some(item.clone());
                break;
            }
        }

        if let Some(item) = to_remove {
            self.tree.remove(&item).is_some()
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.tree = rstar::RTree::new();
    }

    pub fn query_bbox(&self, bbox: &BoundingBox) -> Vec<&IndexedSegment> {
        let lower = [
            bbox.min.x as f64 / 1_000_000.0,
            bbox.min.y as f64 / 1_000_000.0,
        ];
        let upper = [
            bbox.max.x as f64 / 1_000_000.0,
            bbox.max.y as f64 / 1_000_000.0,
        ];
        let envelope = AABB::from_corners(lower, upper);
        self.tree.locate_in_envelope(&envelope).collect()
    }

    pub fn query_radius(&self, x: i64, y: i64, radius_nm: i64) -> Vec<&IndexedSegment> {
        let radius_mm = radius_nm as f64 / 1_000_000.0;
        let x_mm = x as f64 / 1_000_000.0;
        let y_mm = y as f64 / 1_000_000.0;

        self.tree
            .locate_within_distance([x_mm, y_mm], radius_mm * radius_mm)
            .collect()
    }

    /// Build a spatial index directly from physics metadata.
    ///
    /// This ensures the spatial index indices match the island builder's node list:
    /// - SubstrateLayer entries use `SubstrateLayer { index }` where index is the flat
    ///   index into the substrate_layers slice.
    /// - RouteSegment entries use `RouteSegment { net_idx: 0, seg_idx: flat_idx }` where
    ///   flat_idx is the index into the route_segments slice.
    pub fn build_from_physics(
        substrate_layers: &[SubstrateLayerMetadata],
        route_segments: &[RouteSegmentMetadata],
    ) -> Self {
        let mut index = Self::new();

        for (idx, layer) in substrate_layers.iter().enumerate() {
            let bbox = &layer.bbox;
            let width = bbox.max.x - bbox.min.x;
            let thickness = bbox.max.z - bbox.min.z;
            index.insert(IndexedSegment {
                source: SpatialEntitySource::SubstrateLayer { index: idx },
                segment_id: idx,
                net_id: layer.net as usize,
                width_nm: width,
                thickness_nm: thickness,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            });
        }

        for (idx, seg) in route_segments.iter().enumerate() {
            let bbox = &seg.bbox;
            let width = bbox.max.x - bbox.min.x;
            let thickness = bbox.max.z - bbox.min.z;
            index.insert(IndexedSegment {
                source: SpatialEntitySource::RouteSegment { net_idx: 0, seg_idx: idx },
                segment_id: idx,
                net_id: seg.net as usize,
                width_nm: width,
                thickness_nm: thickness,
                start: bbox.min,
                end: bbox.max,
                layer: bbox.min.z,
            });
        }

        index
    }
}

impl Default for DynamicSpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}
