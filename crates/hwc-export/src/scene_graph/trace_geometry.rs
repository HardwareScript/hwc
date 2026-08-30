//! **v0.2.1: TRACE GEOMETRY ENGINE**
//!
//! Proper segment-level 3D geometry generation for routing traces.
//!
//! # FUNDAMENTAL PRINCIPLES
//!
//! 1. **Segment-Level Granularity**: Each LineSegment generates its own 3D geometry
//!    based on its segment type (horizontal trace vs. via).
//!
//! 2. **No Defaults, No Fallbacks**: All Z-coordinates come from either:
//!    - Explicit `layer_z_range` (from stackup)
//!    - Segment's own Z coordinates (for vias)
//!
//!    Never from min/max across all segments!
//!
//! 3. **Type-Driven Geometry**: Horizontal traces and vias have fundamentally
//!    different extrusion models. Use `LineSegment::segment_type()` to dispatch.
//!
//! 4. **Layer Stackup Truth**: Horizontal traces must reference the layer stackup
//!    to determine physical Z bounds. The segment's Z is a centerline reference.
//!
//! # ARCHITECTURE
//!
//! ```text
//! AnalyticTrace (net-level)
//!   ├── Segment 1: Via (Z=1450→1250)      → Extrude as vertical cylinder
//!   ├── Segment 2: Horizontal (Z=1250)    → Lookup layer bounds, extrude
//!   ├── Segment 3: Horizontal (Z=1250)    → Same layer, merge into pool
//!   └── Segment 4: Via (Z=1250→1450)      → Extrude as vertical cylinder
//! ```
//!
//! Each segment type generates geometry independently, then we pool by
//! (material, net, z_range) for efficient Boolean union and extrusion.

use crate::geometry_union::stroke_route_segments;
use crate::scene_graph::types::MeshNode;
use clipper2_rust::{FillRule, Point64};

use hwc_engine::material::MaterialId;
use hwc_engine::netlist::NetId;
use hwc_engine::space::{LineSegment, SegmentType};
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

/// A trace segment with its resolved 3D geometry parameters.
///
/// This is the intermediate representation between `LineSegment` (topological)
/// and the final extruded mesh (geometric).
#[derive(Debug, Clone)]
pub struct GeometrySegment {
    /// Original line segment (for XY path)
    pub segment: LineSegment,
    /// Trace width in nanometers
    pub width_nm: i64,
    /// Physical Z range: (z_min, z_max) in nanometers
    /// For horizontal traces: derived from layer stackup
    /// For vias: derived from start.z and end.z
    pub z_range: (i64, i64),
    /// Material ID
    pub material: MaterialId,
    /// Network ID (for grouping)
    pub net_id: NetId,
    /// Segment classification
    pub segment_type: SegmentType,
}

impl GeometrySegment {
    /// Create a geometry segment from a line segment and trace parameters.
    ///
    /// # Parameters
    ///
    /// - `segment`: The topological line segment
    /// - `width_nm`: Trace width in nanometers
    /// - `thickness_nm`: Trace thickness (for horizontal traces)
    /// - `layer_z_range`: Explicit layer Z bounds (from stackup)
    /// - `material`: Material ID
    /// - `net_id`: Network ID
    ///
    /// # Returns
    ///
    /// `Some(GeometrySegment)` if valid geometry can be generated, `None` otherwise.
    pub fn from_line_segment(
        segment: LineSegment,
        width_nm: i64,
        layer_z_range: Option<(i64, i64)>,
        material: MaterialId,
        net_id: NetId,
    ) -> Option<Self> {
        let segment_type = segment.segment_type();

        // Skip degenerate/invalid segments
        if matches!(segment_type, SegmentType::Point | SegmentType::Invalid) {
                        return None;
        }

        // Determine physical Z range based on segment type
        let z_range = match segment_type {
            SegmentType::HorizontalTrace => {
                // Horizontal trace: MUST use explicit layer bounds from stackup
                // Zero-Fallback Policy: layer_z_range is REQUIRED for all routing traces
                let (z_min, z_max) = layer_z_range.expect(
                    "BUG: Horizontal trace segment must have layer_z_range from stackup. \
                     All routing traces must reference a defined stackup layer with explicit Z bounds."
                );

                // Validate that segment Z is within the layer bounds
                if segment.start.z < z_min || segment.start.z > z_max {
                                    }
                (z_min, z_max)
            }
            SegmentType::Via => {
                // Via: MUST use the routing layer's Z-range to ensure proper merging
                // with horizontal traces in the Boolean union and DXF export.
                //
                // Zero-Fallback Policy: If layer_z_range is missing, this is a compiler bug.
                // Vias on routing layers MUST have their layer Z-range provided.
                let (layer_z_min, layer_z_max) = layer_z_range.expect(
                    "BUG: Via segment must have layer_z_range for proper geometry pooling. \
                     Vias connecting to routing layers must be extruded across the full layer \
                     thickness to merge with horizontal traces.",
                );

                
                (layer_z_min, layer_z_max)
            }
            _ => unreachable!(), // Already filtered above
        };

        Some(Self {
            segment,
            width_nm,
            z_range,
            material,
            net_id,
            segment_type,
        })
    }

    /// Generate a pooling key for grouping segments that can share geometry.
    ///
    /// Segments with the same (material, net, z_range) can be merged into a
    /// single 2D path set, unioned, then extruded together.
    pub fn pool_key(&self) -> GeometryPoolKey {
        GeometryPoolKey {
            z_min: self.z_range.0,
            z_max: self.z_range.1,
            material: self.material,
            net_id: self.net_id,
        }
    }
}

/// Key for grouping segments into geometry pools.
///
/// Segments with identical keys can be:
/// 1. Stroked into 2D paths
/// 2. Unioned together (Boolean OR)
/// 3. Extruded as a single mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeometryPoolKey {
    pub z_min: i64,
    pub z_max: i64,
    pub material: MaterialId,
    pub net_id: NetId,
}

/// A pool of 2D paths that share the same Z-extrusion parameters.
#[derive(Debug, Clone)]
pub struct GeometryPool {
    /// Pooling key
    pub key: GeometryPoolKey,
    /// Accumulated 2D paths (in XY plane, to be extruded along Z)
    pub paths: Vec<Vec<Point64>>,
    /// Contiguous line segments pending stroke
    pub pending_segments: Vec<LineSegment>,
    /// Width of the pending segments
    pub pending_width_nm: i64,
}

impl GeometryPool {
    pub fn new(key: GeometryPoolKey) -> Self {
        Self {
            key,
            paths: Vec::new(),
            pending_segments: Vec::new(),
            pending_width_nm: 0,
        }
    }

    /// Add a geometry segment to this pool.
    pub fn add_segment(&mut self, geom_seg: &GeometrySegment) {
        // If the new segment is contiguous with the last one AND has the same width, append it.
        // Otherwise, flush the pending segments and start a new chain.
        if let Some(last) = self.pending_segments.last() {
            if last.end != geom_seg.segment.start || self.pending_width_nm != geom_seg.width_nm {
                self.flush_pending();
            }
        }
        self.pending_segments.push(geom_seg.segment.clone());
        self.pending_width_nm = geom_seg.width_nm;
    }

    /// Flush pending segments by stroking them into a single path
    pub fn flush_pending(&mut self) {
        if !self.pending_segments.is_empty() {
            let outline = stroke_route_segments(&self.pending_segments, self.pending_width_nm);
            self.paths.extend(outline);
            self.pending_segments.clear();
        }
    }

    /// Generate the final 3D mesh for this pool.
    ///
    /// 1. Union all 2D paths (Boolean OR)
    /// 2. Extrude the result from z_min to z_max
    ///
    /// Returns a list of MeshNode objects (one per contour after union).
    pub fn generate_meshes(
        &mut self,
        material_name: &str,
        view: hwc_engine::space::SpaceView,
    ) -> Vec<MeshNode> {
        self.flush_pending();

        if self.paths.is_empty() {
            return Vec::new();
        }

        eprintln!(
            "[GEOMETRY POOL] Generating mesh for key: material={:?}, net={:?}, Z={}→{}nm ({} paths before union)",
            self.key.material, self.key.net_id, self.key.z_min, self.key.z_max, self.paths.len()
        );

        // Perform 2D Boolean Union
        let unioned = clipper2_rust::union_64(&self.paths, &Vec::new(), FillRule::NonZero);

        eprintln!("[GEOMETRY POOL]   After union: {} contours", unioned.len());

        // Extrude each contour
        let z_min_mm = self.key.z_min as f64 / 1_000_000.0;
        let depth_mm = (self.key.z_max - self.key.z_min) as f64 / 1_000_000.0;

        let mut meshes = Vec::new();

        for (contour_idx, contour) in unioned.iter().enumerate() {
            if contour.len() < 3 {
                continue; // Skip degenerate contours
            }

            let outer_points: Vec<(f64, f64)> = contour
                .iter()
                .map(|pt| (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0))
                .collect();

            let mesh = crate::mesh_extrusion::extrude_polygon_mesh(
                &format!(
                    "TracePool_Net{:?}_Contour{}",
                    self.key.net_id.raw(),
                    contour_idx
                ),
                &outer_points,
                &[],
                z_min_mm,
                depth_mm,
                material_name,
                view,
            );
            meshes.push(mesh);
        }

        eprintln!("[GEOMETRY POOL]   Generated {} meshes", meshes.len());

        meshes
    }
}

/// Convert analytic routes into geometry pools for mesh generation.
///
/// This is the main entry point for the trace geometry engine.
///
/// # Algorithm
///
/// 1. For each AnalyticTrace:
///    a. Convert each LineSegment to a GeometrySegment (with resolved Z bounds)
///    b. Group GeometrySegments by their pool key (material, net, z_range)
/// 2. For each GeometryPool:
///    a. Union all 2D path outlines
///    b. Extrude to create 3D mesh
///
/// # Returns
///
/// A map from GeometryPoolKey to MeshNode list.
pub fn generate_trace_geometry(space: &HardwareSpace) -> FxHashMap<GeometryPoolKey, GeometryPool> {
    let mut pools: FxHashMap<GeometryPoolKey, GeometryPool> = FxHashMap::default();

    // Convert each trace into geometry segments and pool them
    for trace in &space.analytic_routes {
        for segment in trace.segments.iter() {
            // Convert LineSegment to GeometrySegment
            let geom_seg = match GeometrySegment::from_line_segment(
                segment.clone(),
                trace.cross_section.width_nm,
                trace.layer_z_range,
                trace.material,
                trace.net_id,
            ) {
                Some(gs) => gs,
                None => {
                                        continue;
                }
            };

            
            // Add to appropriate pool (horizontal traces only)
            let key = geom_seg.pool_key();
            pools
                .entry(key)
                .or_insert_with(|| GeometryPool::new(key))
                .add_segment(&geom_seg);
        }
    }

    // Flush all pending segments
    for pool in pools.values_mut() {
        pool.flush_pending();
    }

    pools
}
