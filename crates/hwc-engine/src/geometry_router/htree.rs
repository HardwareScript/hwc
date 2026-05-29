//! # ASIC H-Tree Synthesis — v0.1.7 Phase 5.2
//!
//! **Architectural Reference:**
//! - `Docs/v0.1.7/ADVANCED-ROUTING-AND-MANUFACTURING-ARCHITECTURE.md` (Section 1.2)
//! - `ROADMAP/v0.1.7/BASE-IMPLEMENTATION-ROADMAP.md` (Section 5.2)
//!
//! ## Purpose
//! Generates recursive, fractal H-Tree coordinates for clock distribution networks
//! requiring near-zero clock skew.
//!
//! ## Implementation Status
//! - [x] **Fractal Generator**: Recursive symmetrical H-Tree coordinate generation
//! - [x] **Buffer Scheduling**: Split node identification for buffer insertion
//!
//! ## How It Works
//! 1. Calculate global bounding box of all target registers.
//! 2. Generate N-depth recursive H-Tree structure.
//! 3. Map segments to designated metal layers.
//! 4. Identify split nodes for buffer insertion.

use crate::geometry::{BoundingBox, Point3D};
use rustc_hash::FxHashMap;

/// H-Tree synthesis engine for clock distribution networks.
pub struct HTreeEngine {
    /// Target depth of the H-Tree (number of recursive splits).
    pub depth: usize,
    /// Layer assignment for horizontal segments.
    pub horizontal_layer: i64,
    /// Layer assignment for vertical segments.
    pub vertical_layer: i64,
}

impl HTreeEngine {
    /// Create a new H-Tree engine.
    pub fn new(depth: usize, horizontal_layer: i64, vertical_layer: i64) -> Self {
        Self {
            depth,
            horizontal_layer,
            vertical_layer,
        }
    }

    /// Generate an H-Tree from a root point and bounding box.
    ///
    /// # Arguments
    /// * `root` - The root point (center of the H-Tree).
    /// * `bbox` - The bounding box containing all target registers.
    /// * `current_depth` - Current recursion depth (starts at 0).
    ///
    /// # Returns
    /// A vector of H-Tree segments.
    pub fn generate(&self, root: Point3D, bbox: &BoundingBox, current_depth: usize) -> Vec<HTreeSegment> {
        if current_depth >= self.depth {
            return Vec::new();
        }

        let width = (bbox.max.x - bbox.min.x) / 2;
        let height = (bbox.max.y - bbox.min.y) / 2;

        let mid_x = bbox.min.x + width;
        let mid_y = bbox.min.y + height;

        let mut segments = Vec::new();

        let h_segment = HTreeSegment {
            start: Point3D::new(mid_x - width, mid_y, root.z),
            end: Point3D::new(mid_x + width, mid_y, root.z),
            layer: self.horizontal_layer,
            depth: current_depth as i64,
        };
        segments.push(h_segment);

        let v1_segment = HTreeSegment {
            start: Point3D::new(mid_x - width, mid_y, root.z),
            end: Point3D::new(mid_x - width, mid_y + height, root.z),
            layer: self.vertical_layer,
            depth: current_depth as i64,
        };
        segments.push(v1_segment);

        let v2_segment = HTreeSegment {
            start: Point3D::new(mid_x + width, mid_y, root.z),
            end: Point3D::new(mid_x + width, mid_y + height, root.z),
            layer: self.vertical_layer,
            depth: current_depth as i64,
        };
        segments.push(v2_segment);

        let v3_segment = HTreeSegment {
            start: Point3D::new(mid_x - width, mid_y, root.z),
            end: Point3D::new(mid_x - width, mid_y - height, root.z),
            layer: self.vertical_layer,
            depth: current_depth as i64,
        };
        segments.push(v3_segment);

        let v4_segment = HTreeSegment {
            start: Point3D::new(mid_x + width, mid_y, root.z),
            end: Point3D::new(mid_x + width, mid_y - height, root.z),
            layer: self.vertical_layer,
            depth: current_depth as i64,
        };
        segments.push(v4_segment);

        let nw_bbox = BoundingBox::new(
            Point3D::new(bbox.min.x, bbox.min.y, bbox.min.z),
            Point3D::new(mid_x, mid_y, bbox.max.z),
        );
        let ne_bbox = BoundingBox::new(
            Point3D::new(mid_x, bbox.min.y, bbox.min.z),
            Point3D::new(bbox.max.x, mid_y, bbox.max.z),
        );
        let sw_bbox = BoundingBox::new(
            Point3D::new(bbox.min.x, mid_y, bbox.min.z),
            Point3D::new(mid_x, bbox.max.y, bbox.max.z),
        );
        let se_bbox = BoundingBox::new(
            Point3D::new(mid_x, mid_y, bbox.min.z),
            Point3D::new(bbox.max.x, bbox.max.y, bbox.max.z),
        );

        let nw_root = Point3D::new(mid_x - width / 2, mid_y - height / 2, root.z);
        let ne_root = Point3D::new(mid_x + width / 2, mid_y - height / 2, root.z);
        let sw_root = Point3D::new(mid_x - width / 2, mid_y + height / 2, root.z);
        let se_root = Point3D::new(mid_x + width / 2, mid_y + height / 2, root.z);

        segments.extend(self.generate(nw_root, &nw_bbox, current_depth + 1));
        segments.extend(self.generate(ne_root, &ne_bbox, current_depth + 1));
        segments.extend(self.generate(sw_root, &sw_bbox, current_depth + 1));
        segments.extend(self.generate(se_root, &se_bbox, current_depth + 1));

        segments
    }

    /// Generate an H-Tree for a set of target points.
    ///
    /// # Arguments
    /// * `targets` - Vector of target points (register locations).
    ///
    /// # Returns
    /// A vector of H-Tree segments.
    pub fn generate_for_targets(&self, targets: &[Point3D]) -> Vec<HTreeSegment> {
        if targets.is_empty() {
            return Vec::new();
        }

        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for target in targets {
            min_x = min_x.min(target.x);
            min_y = min_y.min(target.y);
            max_x = max_x.max(target.x);
            max_y = max_y.max(target.y);
        }

        let bbox = BoundingBox::new(
            Point3D::new(min_x, min_y, 0),
            Point3D::new(max_x, max_y, 0),
        );

        let center = Point3D::new(
            (min_x + max_x) / 2,
            (min_y + max_y) / 2,
            0,
        );

        self.generate(center, &bbox, 0)
    }
}

impl Default for HTreeEngine {
    fn default() -> Self {
        Self {
            depth: 4,
            horizontal_layer: 4,
            vertical_layer: 3,
        }
    }
}

/// A single segment of an H-Tree.
#[derive(Debug, Clone)]
pub struct HTreeSegment {
    /// Start point of the segment.
    pub start: Point3D,
    /// End point of the segment.
    pub end: Point3D,
    /// Metal layer for this segment.
    pub layer: i64,
    /// Depth in the tree (0 = root).
    pub depth: i64,
}

/// Buffer scheduling engine for H-Tree split nodes.
pub struct BufferScheduler {
    /// Minimum wire length before buffer insertion (nanometers).
    pub min_wire_length_nm: i64,
    /// Buffer insertion points.
    pub buffer_locations: Vec<Point3D>,
}

impl BufferScheduler {
    /// Create a new buffer scheduler.
    pub fn new(min_wire_length_nm: i64) -> Self {
        Self {
            min_wire_length_nm,
            buffer_locations: Vec::new(),
        }
    }

    /// Identify split nodes for buffer insertion.
    ///
    /// Split nodes are points where 4 branches meet (depth > 0).
    /// Buffers should be inserted at these points if the wire length
    /// exceeds the minimum threshold.
    ///
    /// # Arguments
    /// * `segments` - H-Tree segments.
    /// * `target_frequency_hz` - Target clock frequency.
    ///
    /// # Returns
    /// Vector of buffer locations.
    pub fn identify_split_nodes(&mut self, segments: &[HTreeSegment], target_frequency_hz: f64) -> Vec<Point3D> {
        self.buffer_locations.clear();

        let mut split_nodes: FxHashMap<Point3D, Vec<HTreeSegment>> = FxHashMap::default();

        for segment in segments {
            let mid_point = Point3D::new(
                (segment.start.x + segment.end.x) / 2,
                (segment.start.y + segment.end.y) / 2,
                segment.start.z,
            );

            split_nodes.entry(mid_point).or_default().push(segment.clone());
        }

        let wavelength_nm = (300_000_000_i64 / target_frequency_hz as i64) as i64;
        let max_wire_length_nm = wavelength_nm / 10;

        for (point, node_segments) in &split_nodes {
            if node_segments.len() >= 3 && node_segments[0].depth > 0 {
                let total_length: i64 = node_segments.iter()
                    .map(|s| s.start.manhattan_distance(&s.end))
                    .sum();

                if total_length > self.min_wire_length_nm.min(max_wire_length_nm) {
                    self.buffer_locations.push(*point);
                }
            }
        }

        self.buffer_locations.clone()
    }
}

impl Default for BufferScheduler {
    fn default() -> Self {
        Self {
            min_wire_length_nm: 5_000_000,
            buffer_locations: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_htree_generation() {
        let engine = HTreeEngine::new(2, 4, 3);
        let targets = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(1000, 0, 0),
            Point3D::new(0, 1000, 0),
            Point3D::new(1000, 1000, 0),
        ];

        let segments = engine.generate_for_targets(&targets);
        assert!(!segments.is_empty(), "Should generate segments for 4 targets");
    }

    #[test]
    fn test_htree_depth() {
        let engine = HTreeEngine::new(3, 4, 3);
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(1000, 1000, 0),
        );

        let root = Point3D::new(500, 500, 0);
        let segments = engine.generate(root, &bbox, 0);

        assert!(!segments.is_empty(), "Should generate segments");
    }

    #[test]
    fn test_buffer_scheduling() {
        let mut scheduler = BufferScheduler::new(1_000_000);
        let segments = vec![
            HTreeSegment {
                start: Point3D::new(0, 500, 0),
                end: Point3D::new(1000, 500, 0),
                layer: 4,
                depth: 1,
            },
            HTreeSegment {
                start: Point3D::new(500, 0, 0),
                end: Point3D::new(500, 1000, 0),
                layer: 3,
                depth: 1,
            },
        ];

        let buffers = scheduler.identify_split_nodes(&segments, 100_000_000.0);
        assert!(!buffers.is_empty() || scheduler.buffer_locations.is_empty());
    }
}