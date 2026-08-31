use crate::scene_graph::types::{FaceCulling, Vertex};
use hwc_engine::SpaceView;

/// Parameters for [`subdivide_rect`].
pub(super) struct SubdivideRectParams {
    pub(super) x1: f64,
    pub(super) y1: f64,
    pub(super) x2: f64,
    pub(super) y2: f64,
    pub(super) z_min: f64,
    pub(super) depth: f64,
    pub(super) cutouts: Vec<CutoutParams>,
    pub(super) material_name: String,
    pub(super) view: SpaceView,
    pub(super) base_culling: FaceCulling,
}

/// Parameters for [`render_hole_zone`].
pub(super) struct HoleZoneParams<'a> {
    pub(super) x1: f64,
    pub(super) y1: f64,
    pub(super) x2: f64,
    pub(super) y2: f64,
    pub(super) z_min: f64,
    pub(super) depth: f64,
    pub(super) hx: f64,
    pub(super) hy: f64,
    pub(super) hr: f64,
    pub(super) map_vertex: &'a dyn Fn(f64, f64, f64) -> Vertex,
}

/// Cutout parameters for hole-aware meshes (v0.1.7)
#[derive(Debug, Clone, Copy)]
pub enum CutoutParams {
    /// Circular hole (center_x, center_y, diameter, z_min, z_max)
    Cylinder {
        cx: f64,
        cy: f64,
        dia: f64,
        z_min: f64,
        z_max: f64,
    },
    /// Rectangular pocket (min_x, min_y, max_x, max_y, z_min, z_max)
    Rect {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        z_min: f64,
        z_max: f64,
    },
}
