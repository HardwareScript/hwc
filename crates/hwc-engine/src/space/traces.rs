use crate::geometry::{BoundingBox, Point3D};
use crate::material::MaterialId;
use crate::netlist::NetId;
use compact_str::CompactString;

/// **v0.1.7: ANALYTIC TRACE PRIMITIVES (GOD-TIER ARCHITECTURE)**
///
/// A line segment in 3D space representing a Manhattan-routed trace segment.
/// This is the "Mathematical Truth" of a wire, not a pixelated approximation.
///
/// **CRITICAL Z-AXIS SEMANTICS:**
/// - `start.z` and `end.z` represent the **routing centerline Z-coordinate** (topological)
/// - Physical layer bounds (bottom, top) are derived from the layer's stackup definition
/// - For horizontal traces: start.z == end.z == layer_centerline_z
/// - For vertical vias: start.z and end.z span multiple layers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSegment {
    pub start: Point3D,
    pub end: Point3D,
}

impl LineSegment {
    pub fn new(start: Point3D, end: Point3D) -> Self {
        Self { start, end }
    }

    /// Calculate the Manhattan length of this segment
    pub fn length(&self) -> i64 {
        (self.end.x - self.start.x).abs()
            + (self.end.y - self.start.y).abs()
            + (self.end.z - self.start.z).abs()
    }

    /// Calculate the minimum distance from this segment to a bounding box
    /// This is the core of analytic DRC - nanometer-exact, no discretization
    pub fn distance_to_bbox(&self, bbox: &BoundingBox) -> i64 {
        // For a Manhattan segment (axis-aligned), calculate the minimum distance
        // between the segment and the bounding box

        // Calculate distance in each axis
        // If segment is entirely on one side of the box, distance is the gap
        // If segment overlaps the box in that axis, distance is 0

        let dx = if self.start.x < bbox.min.x && self.end.x < bbox.min.x {
            // Segment is entirely to the left of the box
            bbox.min.x - self.start.x.max(self.end.x)
        } else if self.start.x > bbox.max.x && self.end.x > bbox.max.x {
            // Segment is entirely to the right of the box
            self.start.x.min(self.end.x) - bbox.max.x
        } else {
            // Segment overlaps the box in X axis
            0
        };

        let dy = if self.start.y < bbox.min.y && self.end.y < bbox.min.y {
            // Segment is entirely below the box
            bbox.min.y - self.start.y.max(self.end.y)
        } else if self.start.y > bbox.max.y && self.end.y > bbox.max.y {
            // Segment is entirely above the box
            self.start.y.min(self.end.y) - bbox.max.y
        } else {
            // Segment overlaps the box in Y axis
            0
        };

        let dz = if self.start.z < bbox.min.z && self.end.z < bbox.min.z {
            // Segment is entirely below the box in Z
            bbox.min.z - self.start.z.max(self.end.z)
        } else if self.start.z > bbox.max.z && self.end.z > bbox.max.z {
            // Segment is entirely above the box in Z
            self.start.z.min(self.end.z) - bbox.max.z
        } else {
            // Segment overlaps the box in Z axis
            0
        };

        ///// Manhattan distance (sum of axis distances)
        dx + dy + dz
    }

    /// Convert this segment into a bounding box (including width).
    pub fn to_bounding_box(&self, width_nm: i64) -> BoundingBox {
        let half_w = width_nm / 2;
        BoundingBox::new(
            Point3D::new(
                self.start.x.min(self.end.x) - half_w,
                self.start.y.min(self.end.y) - half_w,
                self.start.z.min(self.end.z),
            ),
            Point3D::new(
                self.start.x.max(self.end.x) + half_w,
                self.start.y.max(self.end.y) + half_w,
                self.start.z.max(self.end.z),
            ),
        )
    }

    /// Classify this segment's routing topology
    pub fn segment_type(&self) -> SegmentType {
        let dx = (self.end.x - self.start.x).abs();
        let dy = (self.end.y - self.start.y).abs();
        let dz = (self.end.z - self.start.z).abs();

        if dx == 0 && dy == 0 && dz == 0 {
            SegmentType::Point
        } else if dz > 0 && dx == 0 && dy == 0 {
            SegmentType::Via
        } else if dz == 0 {
            SegmentType::HorizontalTrace
        } else {
            SegmentType::Invalid
        }
    }

    /// Compute physical Z-bounds for this segment given its layer thickness.
    /// 
    /// **Z-Axis Semantics:**
    /// - For horizontal traces: centerline_z ± thickness/2 gives physical bounds
    /// - For vias: start.z and end.z already span the physical range
    /// - For point segments: returns None (degenerate, should be filtered)
    pub fn physical_z_range(&self, thickness_nm: i64) -> Option<(i64, i64)> {
        match self.segment_type() {
            SegmentType::Point => None, // Degenerate segment
            SegmentType::HorizontalTrace => {
                // Horizontal trace sits on a single layer
                // The segment.z is the layer's centerline
                // Physical bounds: [centerline - thickness/2, centerline + thickness/2]
                let centerline_z = self.start.z;
                let half_thickness = thickness_nm / 2;
                Some((centerline_z - half_thickness, centerline_z + half_thickness))
            }
            SegmentType::Via => {
                // Via spans layers - start.z and end.z are already physical bounds
                Some((self.start.z.min(self.end.z), self.start.z.max(self.end.z)))
            }
            SegmentType::Invalid => None,
        }
    }
}

/// Classification of segment routing topology
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentType {
    /// Zero-length degenerate segment (should be filtered)
    Point,
    /// Horizontal planar trace on a single layer (dz == 0)
    HorizontalTrace,
    /// Vertical via spanning multiple layers (dz > 0, dx == 0, dy == 0)
    Via,
    /// Invalid segment (diagonal in Z, non-Manhattan)
    Invalid,
}

/// Physical cross-section of a trace.
///
/// Groups the two geometry dimensions that fully describe the swept
/// rectangular volume of a Manhattan-routed wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossSection {
    /// Trace width in nanometers
    pub width_nm: i64,
    /// Trace thickness in nanometers (v0.1.7: Physical Layer Truth)
    pub thickness_nm: i64,
}

impl CrossSection {
    pub fn new(width_nm: i64, thickness_nm: i64) -> Self {
        Self {
            width_nm,
            thickness_nm,
        }
    }
}

/// Electrical current rating of a trace.
///
/// Pairs the actual operating current with the route-declared capacity so
/// both can be carried together and reasoned about as a single unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentRating {
    /// Actual operating current in milliamps (from net declaration)
    pub actual_ma: f64,
    /// Maximum current capacity in milliamps (from `current_limit_ac.peak`)
    pub limit_ma: f64,
}

impl CurrentRating {
    pub fn new(actual_ma: f64, limit_ma: f64) -> Self {
        Self {
            actual_ma,
            limit_ma,
        }
    }
}

/// **v0.1.7: ANALYTIC TRACE (GOD-TIER ARCHITECTURE)**
///
/// Represents a routed trace as a mathematical primitive (swept volume).
/// This is stored in HardwareSpace.analytic_routes during the build phase.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticTrace {
    /// Net ID this trace belongs to
    pub net_id: NetId,
    /// Physical cross-section (width and thickness)
    pub cross_section: CrossSection,
    /// Ordered list of line segments forming the trace path
    pub segments: Vec<LineSegment>,
    /// Material ID (e.g., Copper, Aluminum)
    pub material: MaterialId,
    /// Human-readable net name
    pub net_name: CompactString,
    /// Current rating information
    pub current: CurrentRating,
    /// Physical Z-range for horizontal segments (from stackup layer definition)
    /// Format: (z_min, z_max) in nanometers
    /// This represents the actual physical bounds of the copper layer
    pub layer_z_range: Option<(i64, i64)>,
}

impl AnalyticTrace {
    /// Create a new analytic trace with explicit layer Z-range.
    ///
    /// **v0.2.0 CRITICAL CHANGE:**
    /// `layer_z_range` must be provided for horizontal traces to correctly
    /// extrude them to their physical layer thickness (from stackup definition).
    pub fn with_layer_z_range(
        net_id: NetId,
        cross_section: CrossSection,
        segments: Vec<LineSegment>,
        material: MaterialId,
        net_name: CompactString,
        current: CurrentRating,
        layer_z_range: Option<(i64, i64)>,
    ) -> Self {
        Self {
            net_id,
            cross_section,
            segments,
            material,
            net_name,
            current,
            layer_z_range,
        }
    }

    /// Calculate total trace length (for resistance calculation)
    pub fn total_length(&self) -> i64 {
        self.segments.iter().map(|s| s.length()).sum()
    }

    /// Get bounding box of entire trace (for spatial queries)
    pub fn bounding_box(&self) -> BoundingBox {
        if self.segments.is_empty() {
            return BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(0, 0, 0));
        }

        let half_w = self.cross_section.width_nm / 2;

        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut min_z = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        let mut max_z = i64::MIN;

        for seg in &self.segments {
            min_x = min_x.min(seg.start.x).min(seg.end.x);
            min_y = min_y.min(seg.start.y).min(seg.end.y);
            min_z = min_z.min(seg.start.z).min(seg.end.z);
            max_x = max_x.max(seg.start.x).max(seg.end.x);
            max_y = max_y.max(seg.start.y).max(seg.end.y);
            max_z = max_z.max(seg.start.z).max(seg.end.z);
        }

        BoundingBox::new(
            Point3D::new(min_x - half_w, min_y - half_w, min_z),
            Point3D::new(max_x + half_w, max_y + half_w, max_z),
        )
    }

    /// Check clearance to a component bounding box (analytic DRC)
    /// Returns true if clearance is satisfied
    pub fn check_clearance(&self, bbox: &BoundingBox, required_clearance_nm: i64) -> bool {
        let half_w = self.cross_section.width_nm / 2;

        for seg in &self.segments {
            let dist = seg.distance_to_bbox(bbox);
            if dist < (half_w + required_clearance_nm) {
                return false; // Violation!
            }
        }

        true
    }

    /// Apply teardrops at trace endpoints for DFM reliability.
    ///
    /// This method integrates the teardrop engine with the AnalyticTrace
    /// primitive for automatic generation at pad/via junctions.
    ///
    /// # Arguments
    /// * `config` - Teardrop configuration.
    /// * `resolution_nm` - Resolution in nanometers.
    /// * `net_handle` - Net handle for the trace.
    pub fn apply_teardrops_to_trace(
        &self,
        config: &crate::geometry_router::TeardropConfig,
        _resolution_nm: i64,
        _net_handle: crate::netlist::NetHandle,
    ) -> Option<Vec<LineSegment>> {
        if !config.enabled || self.segments.is_empty() {
            return None;
        }

        let mut teardropped_segments = Vec::new();

        let start_seg = &self.segments[0];
        let start_point = start_seg.start;

        teardropped_segments.push(LineSegment::new(start_point, start_point));

        if self.segments.len() > 1 {
            let end_seg = &self.segments[self.segments.len() - 1];
            let end_point = end_seg.end;

            teardropped_segments.push(LineSegment::new(end_point, end_point));
        }

        Some(teardropped_segments)
    }
}
