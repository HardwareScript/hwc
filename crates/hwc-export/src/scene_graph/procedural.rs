//! Procedural component geometry generation (Limitation 6)

use super::types::{MeshNode, BoxParams, FaceCulling};
use super::mesh_generation::create_box_mesh;
use hwc_engine::SpaceView;

/// Generate procedural meshes for a TO-220 package.
/// 
/// Returns a list of meshes with different materials (Plastic, Metal).
pub fn create_to220_meshes(
    name: &str,
    center: (f64, f64, f64),
    _rotation_deg: f64, // TODO: Implement rotation
    view: SpaceView,
) -> Vec<MeshNode> {
    let (cx, cy, cz) = center;
    let mut meshes = Vec::new();

    // TO-220 Dimensions (Standard approximate in mm)
    // Vertical Orientation (Z-up in engine)
    let body_w = 10.0;
    let body_h = 4.5;  // Thickness (Engine Y)
    let body_z = 9.0;  // Height (Engine Z)
    let tab_z = 15.0;  // Total height including tab (Engine Z)
    let tab_thick = 1.3;
    let pin_z_len = 10.0; // Length of pins (Engine Z)
    
    // ADJUSTMENT: The 'cz' passed from SceneGraph is the center of the component bbox.
    // For procedural meshes to sit correctly, we need to offset from this center.
    // Total component height = pin_z_len + tab_z
    let total_height = pin_z_len + tab_z;
    let bottom_z = cz - total_height / 2.0;

    // 1. Plastic Body (Black)
    // Sits above the pins
    let body_params = BoxParams {
        x: cx - body_w / 2.0,
        y: cy - body_h / 2.0,
        z: bottom_z + pin_z_len,
        width: body_w,
        height: body_h,
        depth: body_z,
    };
    meshes.push(create_box_mesh(&format!("{}_body", name), body_params, "Component", view, FaceCulling::none()));

    // 2. Metal Tab (Silver)
    // Overlap slightly with the body to prevent the "slit" while avoiding Z-fighting
    let overlap_offset = 0.01; // 10um overlap
    let tab_params = BoxParams {
        x: cx - body_w / 2.0,
        y: cy + body_h / 2.0 - tab_thick + overlap_offset, // Sits inside the body by 10um
        z: bottom_z + pin_z_len,
        width: body_w,
        height: tab_thick,
        depth: tab_z,
    };
    meshes.push(create_box_mesh(&format!("{}_tab", name), tab_params, "Silver", view, FaceCulling::none()));

    // 3. Pins (Silver)
    let pin_w = 0.8;
    let pin_h = 0.5;
    let pin_pitch = 2.54;

    for i in 0..3 {
        let px = cx + (i as f64 - 1.0) * pin_pitch;
        let pin_params = BoxParams {
            x: px - pin_w / 2.0,
            y: cy - pin_h / 2.0,
            z: bottom_z,
            width: pin_w,
            height: pin_h,
            depth: pin_z_len,
        };
        meshes.push(create_box_mesh(&format!("{}_pin_{}", name, i), pin_params, "Silver", view, FaceCulling::none()));
    }

    meshes
}
