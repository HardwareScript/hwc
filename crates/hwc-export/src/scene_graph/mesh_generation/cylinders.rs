use crate::scene_graph::types::{Face, FaceCulling, MeshNode, Vertex};
use hwc_engine::SpaceView;

/// Create a cylindrical mesh with fully triangulated top/bottom solid caps
#[allow(clippy::too_many_arguments)]
pub fn create_cylinder_mesh(
    name: &str,
    center: (f64, f64, f64),
    diameter: f64,
    height: f64,
    segments: u32,
    material_name: &str,
    view: SpaceView,
    culling: FaceCulling,
) -> MeshNode {
    let (cx, cy, cz) = center;
    let radius = diameter / 2.0;
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Helper to map 2D coordinates to 3D scene space based on active view
    let map_vertex = |ex: f64, ey: f64, ez: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex {
                x: ex,
                y: ez,
                z: ey,
            },
            SpaceView::Vertical => Vertex {
                x: ex,
                y: ey,
                z: ez,
            },
        }
    };

    let actual_segments = if segments == 16 { 64 } else { segments };

    // 1. Generate vertices for top and bottom rings
    for i in 0..actual_segments {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let dx = radius * angle.cos();
        let dy = radius * angle.sin();

        // Bottom vertex: Index i * 2
        vertices.push(map_vertex(cx + dx, cy + dy, cz));
        // Top vertex: Index i * 2 + 1
        vertices.push(map_vertex(cx + dx, cy + dy, cz + height));
    }

    // 2. Add center vertices for triangulated solid caps
    let bottom_center_idx = vertices.len();
    vertices.push(map_vertex(cx, cy, cz));
    let top_center_idx = vertices.len();
    vertices.push(map_vertex(cx, cy, cz + height));

    // 3. Generate side walls (Quads) and triangulate caps
    for i in 0..actual_segments as usize {
        let next = (i + 1) % actual_segments as usize;
        let b1 = i * 2;
        let t1 = i * 2 + 1;
        let b2 = next * 2;
        let t2 = next * 2 + 1;

        // Side walls (Quads)
        faces.push(Face {
            vertices: vec![b1, b2, t2, t1],
        });

        // v0.1.8 FIXED: Triangulate bottom cap using a triangle fan (facing down)
        if !culling.bottom {
            faces.push(Face {
                vertices: vec![bottom_center_idx, b2, b1],
            });
        }

        // v0.1.8 FIXED: Triangulate top cap using a triangle fan (facing up)
        if !culling.top {
            faces.push(Face {
                vertices: vec![top_center_idx, t1, t2],
            });
        }
    }

    MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    }
}
