use crate::geometry::{BoundingBox, Point3D};
use compact_str::CompactString;
use crate::connectivity::DeviceBinding;

/// A Planar Island represents a continuous copper region on a single layer.
///
/// Each unioned 2D contour is treated as a discrete Planar Island. Islands are
/// stored with their Z-interval metadata. Everything within a single island's
/// boundary is electrically contiguous.
///
/// This is the fundamental node type in the PIVB connectivity graph.
#[derive(Debug, Clone)]
pub struct PlanarIsland {
    /// Unique island identifier
    pub id: usize,
    /// Layer name (e.g., "top_copper", "metal1")
    pub layer_name: CompactString,
    /// Minimum Z coordinate of this island's layer interval
    pub z_min: i64,
    /// Maximum Z coordinate of this island's layer interval
    pub z_max: i64,
    /// Pre-welded boundary contour (axis-aligned bounding box of the contour)
    pub boundary: BoundingBox,
    /// Fast rejection bounding box
    pub bbox: BoundingBox,
    /// Center point for diagnostic reporting
    pub center: Point3D,
    /// Net name this island belongs to
    pub net_name: CompactString,
    /// Net ID (strongly-typed)
    pub net_id: hwc_types::NetId,
    /// Material ID
    pub material: u8,
    /// Device terminal binding (v0.2.1) - if present, this island is part of a device terminal
    pub device_binding: Option<DeviceBinding>,
}

/// A Vertical Bridge represents a via or contact that connects Planar Islands
/// across different Z-layers.
///
/// This is the fundamental edge type in the PIVB connectivity graph.
#[derive(Debug, Clone, Copy)]
pub struct VerticalBridge {
    /// Via/contact identifier
    pub id: usize,
    /// The island ID on layer A that this bridge connects to
    pub island_a: usize,
    /// The island ID on layer B that this bridge connects to
    pub island_b: usize,
    /// X coordinate of the via center
    pub x: i64,
    /// Y coordinate of the via center
    pub y: i64,
    /// Z-min of the via span
    pub z_min: i64,
    /// Z-max of the via span
    pub z_max: i64,
}

/// Diagnostic report for a fragmented net.
///
/// Generated when the PIVB Solver detects that a net has more than one
/// connected component. Provides structured island-level diagnostics.
#[derive(Debug, Clone)]
pub struct FragmentationReport {
    /// The net name that is fragmented
    pub net_name: CompactString,
    /// Number of disconnected components
    pub component_count: usize,
    /// Each disconnected island group
    pub islands: Vec<FragmentedIsland>,
    /// Suggested fix for bridging the gap
    pub suggested_fix: CompactString,
}

/// A single disconnected island in a fragmentation report.
#[derive(Debug, Clone)]
pub struct FragmentedIsland {
    /// Island group index (0-based)
    pub group_index: usize,
    /// Number of PlanarIslands in this component
    pub island_count: usize,
    /// Representative bounding box of the component
    pub bbox: BoundingBox,
    /// Center coordinate for viewport focus
    pub center: Point3D,
    /// Layers represented in this component
    pub layers: Vec<CompactString>,
}

/// Result of PIVB connectivity validation.
#[derive(Debug, Clone)]
pub enum ConnectivityResult {
    /// Net is physically continuous (single connected component)
    Pass {
        net_name: CompactString,
        island_count: usize,
        bridge_count: usize,
    },
    /// Net is fragmented (multiple connected components)
    Fail(FragmentationReport),
}

impl ConnectivityResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, ConnectivityResult::Pass { .. })
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, ConnectivityResult::Fail(_))
    }
}
