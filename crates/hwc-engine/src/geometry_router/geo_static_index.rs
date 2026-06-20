use crate::geometry::BoundingBox;

use super::spatial_index::IndexedSegment;

pub struct StaticLayerIndex {
    segments: Vec<IndexedSegment>,
    sorted_by_x_min: Vec<usize>,
}

impl StaticLayerIndex {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            sorted_by_x_min: Vec::new(),
        }
    }

    pub fn build(segments: Vec<IndexedSegment>) -> Self {
        let sorted_by_x_min: Vec<usize> = (0..segments.len()).collect();
        let mut index = Self {
            segments,
            sorted_by_x_min,
        };
        index
            .sorted_by_x_min
            .sort_by_key(|&i| index.segments[i].start.x.min(index.segments[i].end.x));
        index
    }

    pub fn query_bbox(&self, bbox: &BoundingBox) -> Vec<&IndexedSegment> {
        let min_x = bbox.min.x;
        let max_x = bbox.max.x;
        let min_y = bbox.min.y;
        let max_y = bbox.max.y;

        let start_idx = self
            .sorted_by_x_min
            .partition_point(|&i| self.segments[i].start.x.min(self.segments[i].end.x) < min_x);

        let mut results = Vec::new();
        for &i in &self.sorted_by_x_min[start_idx..] {
            let seg = &self.segments[i];
            let seg_min_x = seg.start.x.min(seg.end.x) - seg.width_nm / 2;
            let _seg_max_x = seg.start.x.max(seg.end.x) + seg.width_nm / 2;
            if seg_min_x > max_x {
                break;
            }
            let seg_min_y = seg.start.y.min(seg.end.y) - seg.width_nm / 2;
            let seg_max_y = seg.start.y.max(seg.end.y) + seg.width_nm / 2;
            if seg_min_y <= max_y && seg_max_y >= min_y {
                results.push(seg);
            }
        }
        results
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

impl Default for StaticLayerIndex {
    fn default() -> Self {
        Self::new()
    }
}
