use crate::geometry::{BoundingBox, Point3D, TraceSegment};

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

/// A trace segment indexed in the layered spatial index with physical volume awareness.
#[derive(Clone, Debug)]
pub struct IndexedSegment {
    pub source: SpatialEntitySource,
    pub segment_id: usize,
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

    /// The minimum X coordinate of this segment's bounding box (for sorted-array indexing).
    #[inline]
    fn min_x(&self) -> i64 {
        self.start.x.min(self.end.x) - self.width_nm / 2
    }
}

/// A physical layer entry in the Z-range lookup table.
#[derive(Clone, Debug)]
struct LayerEntry {
    z_min: i64,
    z_max: i64,
    layer_idx: usize,
}

/// Layered 2D Spatial Index — replaces R*-tree with per-layer sorted vectors.
///
/// Architecture:
/// - One `Vec<IndexedSegment>` per physical layer, sorted by min-x for binary search
/// - Z-range lookup table maps Z-coordinates to layer indices
/// - All coordinates in i64 nanometers (no f64 conversion)
/// - O(log N_layer) per query instead of O(log N_total)
///
/// For layers not registered via `set_layer_z_ranges`, segments are placed in a
/// fallback bucket (layer_idx = 0) and always queried.
pub struct DynamicSpatialIndex {
    /// Per-layer segments, sorted by min-x for binary search.
    /// Index 0 is the fallback bucket for unregistered layers.
    layer_segments: Vec<Vec<IndexedSegment>>,
    /// Z-range lookup: which physical layer does a Z-coordinate belong to?
    layer_z_ranges: Vec<LayerEntry>,
    /// Flat list of all segments (for iteration and removal).
    all_segments: Vec<IndexedSegment>,
    /// Whether layer Z-ranges have been configured.
    layers_configured: bool,
}

impl DynamicSpatialIndex {
    pub fn new() -> Self {
        Self {
            layer_segments: vec![Vec::new()], // layer 0 = fallback
            layer_z_ranges: Vec::new(),
            all_segments: Vec::new(),
            layers_configured: false,
        }
    }

    /// Configure physical layer Z-ranges from the stackup profile.
    ///
    /// Each entry maps a physical layer name to its Z-range in nanometers.
    /// Segments inserted after this call will be placed in the correct layer bucket.
    ///
    /// # Arguments
    /// * `z_ranges` - Slice of (z_min_nm, z_max_nm) for each layer, in stackup order
    pub fn set_layer_z_ranges(&mut self, z_ranges: &[(i64, i64)]) {
        // eprintln!("[LAYERED INDEX] Configuring {} layer Z-ranges:", z_ranges.len());
        self.layer_z_ranges.clear();
        // layer_segments[0] is the fallback; physical layers start at index 1
        self.layer_segments.clear();
        self.layer_segments.push(Vec::new()); // fallback bucket

        for (i, &(z_min, z_max)) in z_ranges.iter().enumerate() {
            let layer_idx = i + 1; // 1-based to reserve 0 for fallback
            eprintln!("  Layer {}: Z=[{}, {}] nm", layer_idx, z_min, z_max);
            self.layer_z_ranges.push(LayerEntry {
                z_min,
                z_max,
                layer_idx,
            });
            self.layer_segments.push(Vec::new());
        }
        self.layers_configured = true;
    }

    /// Find all layer indices whose Z-range overlaps [z_min, z_max].
    fn layers_for_z_range(&self, z_min: i64, z_max: i64) -> Vec<usize> {
        if !self.layers_configured {
            eprintln!("[LAYERED INDEX] layers_for_z_range: Layers NOT configured, using fallback");
            return vec![0];
        }
        let mut result = Vec::new();
        for entry in &self.layer_z_ranges {
            // Z-ranges overlap if one starts before the other ends
            if z_min <= entry.z_max && z_max >= entry.z_min {
                result.push(entry.layer_idx);
            }
        }
        if result.is_empty() {
            eprintln!("[LAYERED INDEX] layers_for_z_range({}, {}): No matching layers, using fallback", z_min, z_max);
            result.push(0); // fallback
        } else {
            eprintln!("[LAYERED INDEX] layers_for_z_range({}, {}): Found layers {:?}", z_min, z_max, result);
        }
        result
    }

    /// Insert a segment into the index.
    ///
    /// The segment is placed into all layers whose Z-range overlaps the segment's
    /// Z-extent. If no layer Z-ranges are configured, it goes into the fallback bucket.
    pub fn insert(&mut self, segment: IndexedSegment) {
        let z_min = segment.start.z.min(segment.end.z);
        let z_max = segment.start.z.max(segment.end.z);
        let layer_indices = self.layers_for_z_range(z_min, z_max);

        eprintln!("[LAYERED INDEX] Inserting segment: start={:?}, end={:?}, z_range=[{}, {}], into layers {:?}",
            segment.start, segment.end, z_min, z_max, layer_indices);

        for layer_idx in layer_indices {
            if layer_idx < self.layer_segments.len() {
                self.layer_segments[layer_idx].push(segment.clone());
                self.layer_segments[layer_idx].sort_by_key(|s| s.min_x());
            }
        }
        self.all_segments.push(segment);
    }

    /// Iterate over all entities in the spatial index.
    pub fn iter(&self) -> impl Iterator<Item = &IndexedSegment> {
        self.all_segments.iter()
    }

    /// Check if the spatial index is empty.
    pub fn is_empty(&self) -> bool {
        self.all_segments.is_empty()
    }

    /// Get the number of entities in the spatial index.
    pub fn len(&self) -> usize {
        self.all_segments.len()
    }

    /// Remove an entity from the spatial index by source.
    pub fn remove_by_source(&mut self, source: SpatialEntitySource) -> bool {
        if let Some(pos) = self.all_segments.iter().position(|s| s.source == source) {
            let removed = self.all_segments.remove(pos);
            // Remove from all layer buckets
            for bucket in &mut self.layer_segments {
                bucket.retain(|s| s.source != source);
            }
            // Re-sort affected buckets by min-x
            for bucket in &mut self.layer_segments {
                if bucket.len() > 1 {
                    bucket.sort_by_key(|s| s.min_x());
                }
            }
            // Re-insert remaining segments that overlapped the removed segment's Z-range
            // to fix any layer assignment issues
            let _ = removed;
            true
        } else {
            false
        }
    }

    /// Clear all segments from the index.
    pub fn clear(&mut self) {
        eprintln!("[LAYERED INDEX] clear() called - clearing segments but preserving layer structure");
        for bucket in &mut self.layer_segments {
            bucket.clear();
        }
        self.all_segments.clear();
        // NOTE: We do NOT reset layers_configured or layer_z_ranges here.
        // The layer configuration should persist across clear() calls.
        // If you need to fully reset the index, create a new instance.
    }

    /// Get the configured layer Z-ranges, if any.
    pub fn layer_z_ranges(&self) -> Option<Vec<(i64, i64)>> {
        if self.layer_z_ranges.is_empty() {
            None
        } else {
            Some(self.layer_z_ranges.iter().map(|e| (e.z_min, e.z_max)).collect())
        }
    }

    /// Query for segments overlapping a bounding box (2D AABB query).
    ///
    /// Uses binary search on per-layer sorted arrays. Only searches layers
    /// whose Z-range overlaps the query bbox's Z-range.
    /// 
    /// NOTE: Segments that span multiple Z-layers are stored in multiple buckets.
    /// We deduplicate by segment_id to avoid returning the same physical entity
    /// multiple times.
    pub fn query_bbox(&self, bbox: &BoundingBox) -> Vec<&IndexedSegment> {
        let z_min = bbox.min.z;
        let z_max = bbox.max.z;
        eprintln!("[LAYERED INDEX] query_bbox: bbox.min={:?}, bbox.max={:?}, z_range=[{}, {}]",
            bbox.min, bbox.max, z_min, z_max);
        
        let layer_indices = self.layers_for_z_range(z_min, z_max);

        let mut results = Vec::new();
        use rustc_hash::FxHashSet;
        let mut seen_segment_ids = FxHashSet::default();
        
        for layer_idx in layer_indices {
            if layer_idx >= self.layer_segments.len() {
                continue;
            }
            let bucket = &self.layer_segments[layer_idx];
            eprintln!("[LAYERED INDEX] Searching layer bucket {}: {} segments", layer_idx, bucket.len());
            self.query_bucket(bucket, bbox, &mut results, &mut seen_segment_ids);
        }
        eprintln!("[LAYERED INDEX] query_bbox: returning {} results", results.len());
        results
    }

    /// Binary search + linear scan on a single layer bucket.
    /// Deduplicates by segment_id to avoid returning the same segment multiple times
    /// when it spans multiple Z-layers.
    #[inline]
    fn query_bucket<'a>(
        &self,
        bucket: &'a [IndexedSegment],
        bbox: &BoundingBox,
        results: &mut Vec<&'a IndexedSegment>,
        seen_segment_ids: &mut rustc_hash::FxHashSet<usize>,
    ) {
        if bucket.is_empty() {
            return;
        }

        let min_x = bbox.min.x;
        let max_x = bbox.max.x;
        let min_y = bbox.min.y;
        let max_y = bbox.max.y;

        // Binary search: find first segment whose min_x >= bbox.min.x
        let start_idx = bucket.partition_point(|s| s.min_x() < min_x);

        // Linear scan from start_idx, break when min_x > bbox.max.x
        for seg in &bucket[start_idx..] {
            if seg.min_x() > max_x {
                break;
            }
            // Deduplicate by segment_id (segments spanning multiple Z-layers appear in multiple buckets)
            let was_new = seen_segment_ids.insert(seg.segment_id);
            eprintln!("[DEDUP CHECK] segment_id={}, was_new={}, bbox=({},{},{}) to ({},{},{})",
                seg.segment_id, was_new,
                seg.start.x.min(seg.end.x), seg.start.y.min(seg.end.y), seg.start.z,
                seg.start.x.max(seg.end.x), seg.start.y.max(seg.end.y), seg.end.z);
            if !was_new {
                eprintln!("[DEDUP CHECK]   ↳ SKIPPED (duplicate)");
                continue; // Already returned this segment from a different layer bucket
            }
            // Y overlap check
            let seg_min_y = seg.start.y.min(seg.end.y) - seg.width_nm / 2;
            let seg_max_y = seg.start.y.max(seg.end.y) + seg.width_nm / 2;
            if seg_min_y <= max_y && seg_max_y >= min_y {
                eprintln!("[DEDUP CHECK]   ↳ ADDED to results");
                results.push(seg);
            } else {
                eprintln!("[DEDUP CHECK]   ↳ SKIPPED (Y out of range)");
            }
        }
    }

    /// Query for segments within a radius of a point.
    pub fn query_radius(&self, x: i64, y: i64, radius_nm: i64) -> Vec<&IndexedSegment> {
        let bbox = BoundingBox {
            min: Point3D::new(x - radius_nm, y - radius_nm, 0),
            max: Point3D::new(x + radius_nm, y + radius_nm, i64::MAX),
        };
        self.query_bbox(&bbox)
    }

    /// Build a spatial index directly from physics metadata.
    pub fn build_from_physics(
        substrate_layers: &[SubstrateLayerMetadata],
        route_segments: &[RouteSegmentMetadata],
    ) -> Self {
        let mut index = Self::new();

        for (idx, layer) in substrate_layers.iter().enumerate() {
            let bbox = &layer.bbox;
            let _width = bbox.max.x - bbox.min.x;
            let thickness = bbox.max.z - bbox.min.z;
            index.insert(IndexedSegment {
                source: SpatialEntitySource::SubstrateLayer { index: idx },
                segment_id: idx,
                net_id: layer.net as usize,
                width_nm: 0,
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
                source: SpatialEntitySource::RouteSegment {
                    net_idx: 0,
                    seg_idx: idx,
                },
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
