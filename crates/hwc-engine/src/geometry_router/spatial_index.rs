use crate::geometry::{BoundingBox, Point3D, TraceSegment};
use rstar::{AABB, PointDistance, RTreeObject};

/// A trace segment indexed in the R*-tree with physical volume awareness.
/// The bounding box is inflated by half the trace width to represent
/// the actual physical copper volume of the trace.
#[derive(Clone, Debug)]
pub struct IndexedSegment {
    pub segment_id: usize,
    pub net_id: usize,
    pub width_nm: i64,
    pub start: Point3D,
    pub end: Point3D,
    pub layer: i64,
}

impl PartialEq for IndexedSegment {
    fn eq(&self, other: &Self) -> bool {
        self.segment_id == other.segment_id
    }
}

impl IndexedSegment {
    pub fn new(segment_id: usize, net_id: usize, segment: &TraceSegment, layer: i64) -> Self {
        Self {
            segment_id,
            net_id,
            width_nm: segment.width_nm,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2Df64 {
    pub x: f64,
    pub y: f64,
}

impl rstar::Point for Point2Df64 {
    type Scalar = f64;

    const DIMENSIONS: usize = 2;

    fn generate(mut generator: impl FnMut(usize) -> f64) -> Self {
        Self {
            x: generator(0),
            y: generator(1),
        }
    }

    fn nth(&self, index: usize) -> f64 {
        match index {
            0 => self.x,
            1 => self.y,
            _ => panic!("Index out of bounds"),
        }
    }

    fn nth_mut(&mut self, index: usize) -> &mut f64 {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("Index out of bounds"),
        }
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

        let lower = [min_x, min_y];
        let upper = [max_x, max_y];
        AABB::from_corners(lower, upper)
    }
}

impl PointDistance for IndexedSegment {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let center = self.center();
        let cx = center.x as f64 / 1_000_000.0;
        let cy = center.y as f64 / 1_000_000.0;
        let dx = point[0] - cx;
        let dy = point[1] - cy;
        dx * dx + dy * dy
    }
}

/// A dynamic spatial index backed by an R*-tree.
/// Used for macro-placement during floorplanning where
/// dynamic insertion and movement occur frequently.
pub struct DynamicSpatialIndex {
    tree: rstar::RTree<IndexedSegment>,
}

impl DynamicSpatialIndex {
    pub fn new() -> Self {
        Self {
            tree: rstar::RTree::new(),
        }
    }

    pub fn insert(&mut self, segment: IndexedSegment) {
        self.tree.insert(segment);
    }

    pub fn remove(&mut self, segment_id: usize) -> Option<IndexedSegment> {
        let dummy = IndexedSegment {
            segment_id,
            net_id: 0,
            width_nm: 0,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(0, 0, 0),
            layer: 0,
        };
        self.tree.remove(&dummy)
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

    pub fn query_radius(&self, x_nm: i64, y_nm: i64, radius_nm: i64) -> Vec<&IndexedSegment> {
        let point = [x_nm as f64 / 1_000_000.0, y_nm as f64 / 1_000_000.0];
        let radius = radius_nm as f64 / 1_000_000.0;
        self.tree.locate_within_distance(point, radius).collect()
    }

    pub fn query_nearest(&self, x_nm: i64, y_nm: i64) -> Option<(&IndexedSegment, f64)> {
        let point = [x_nm as f64 / 1_000_000.0, y_nm as f64 / 1_000_000.0];
        let nearest = self.tree.nearest_neighbor(&point)?;
        let dist = ((nearest.center().x - x_nm).pow(2) + (nearest.center().y - y_nm).pow(2))
            as f64;
        Some((nearest, dist.sqrt()))
    }

    pub fn clear(&mut self) {
        self.tree = rstar::RTree::new();
    }

    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}

impl Default for DynamicSpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Query the index for all segments that overlap a given segment's bounding box.
/// Excludes segments with the same net_id (same-net segments are handled separately).
pub fn query_overlapping_segments<'a>(
    index: &'a DynamicSpatialIndex,
    segment: &IndexedSegment,
    clearance_nm: i64,
) -> Vec<&'a IndexedSegment> {
    let expanded_bbox = BoundingBox {
        min: Point3D::new(
            segment.start.x.min(segment.end.x) - segment.width_nm / 2 - clearance_nm,
            segment.start.y.min(segment.end.y) - segment.width_nm / 2 - clearance_nm,
            segment.start.z.min(segment.end.z),
        ),
        max: Point3D::new(
            segment.start.x.max(segment.end.x) + segment.width_nm / 2 + clearance_nm,
            segment.start.y.max(segment.end.y) + segment.width_nm / 2 + clearance_nm,
            segment.start.z.max(segment.end.z),
        ),
    };

    index
        .query_bbox(&expanded_bbox)
        .into_iter()
        .filter(|s| s.net_id != segment.net_id)
        .collect()
}
