use crate::scene_graph::types::{BoxParams, FaceCulling, MeshNode};
use crate::scene_graph::mesh_generation::boxes::{self, CutoutParams};
use hwc_engine::SpaceView;

/// Create a box with circular holes mesh (v0.1.7 Limitation 7)
pub fn create_box_with_holes_mesh(
    name: &str,
    params: BoxParams,
    cutouts: Vec<CutoutParams>,
    material_name: &str,
    view: SpaceView,
    culling: FaceCulling,
) -> MeshNode {
    if cutouts.is_empty() {
        return boxes::standard::create_box_mesh(name, params, material_name, view, culling);
    }

    let mut root_mesh = MeshNode {
        name: name.into(),
        vertices: Vec::new(),
        faces: Vec::new(),
        material_name: material_name.into(),
    };

    boxes::subdivision::subdivide_rect(
        &mut root_mesh,
        boxes::params::SubdivideRectParams {
            x1: params.x,
            y1: params.y,
            x2: params.x + params.width,
            y2: params.y + params.height,
            z_min: params.z,
            depth: params.depth,
            cutouts,
            material_name: material_name.to_string(),
            view,
            base_culling: culling,
        },
    );

    root_mesh
}
