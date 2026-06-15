use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;

/// Layer transition information for a net.
#[derive(Debug, Clone)]
pub(crate) struct LayerTransition {
    pub(crate) net_name: CompactString,
    pub(crate) from_layer: usize,
    pub(crate) to_layer: usize,
    /// Physical Z (nm) at the bottom of the lower pour.
    pub(crate) from_z_nm: i64,
    /// Physical Z (nm) at the top of the upper pour.
    pub(crate) to_z_nm: i64,
    pub(crate) from_pour: CompactString,
    pub(crate) to_pour: CompactString,
    pub(crate) from_material: CompactString,
    pub(crate) to_material: CompactString,
    pub(crate) from_bbox: BoundingBox,
    pub(crate) to_bbox: BoundingBox,
    /// Semantic layer names (e.g. "m1", "m2") for correct elevation resolution.
    pub(crate) from_layer_name: Option<CompactString>,
    pub(crate) to_layer_name: Option<CompactString>,
}

/// Overlap region between two pours on different layers.
#[derive(Debug, Clone)]
pub(crate) struct OverlapRegion {
    pub(crate) bbox: BoundingBox,
    pub(crate) center_x_nm: i64,
    pub(crate) center_y_nm: i64,
}

/// Via array configuration for high-current nets.
#[derive(Debug, Clone)]
pub(crate) struct ViaArrayConfig {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) pitch_x_nm: i64,
    pub(crate) pitch_y_nm: i64,
    pub(crate) start_x_nm: i64,
    pub(crate) start_y_nm: i64,
}
