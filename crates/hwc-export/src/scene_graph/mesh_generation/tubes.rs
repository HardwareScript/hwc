use crate::scene_graph::types::{Face, MeshNode, Vertex};
use hwc_engine::SpaceView;

/// Parameters for [`create_tube_mesh`].
pub struct TubeMeshParams {
    pub name: String,
    pub center: (f64, f64, f64),
    pub outer_diameter: f64,
    pub inner_diameter: f64,
    pub height: f64,
    pub segments: u32,
    pub caps: bool,
    pub material_name: String,
    pub view: SpaceView,
}

/// Create a tube (hollow cylinder) mesh (v0.1.7 Limitation 7)
pub fn create_tube_mesh(params: TubeMeshParams) -> MeshNode {
    let TubeMeshParams {
        name,
        center,
        outer_diameter,
        inner_diameter,
        height,
        segments,
        caps,
        material_name,
        view,
    } = params;
    let (cx, cy, mut cz) = center;
    let mut actual_height = height;

    // Apply 1μm Epsilon Offset to prevent Z-fighting with pad surfaces
    if actual_height > 0.002 {
        cz += 0.001;
        actual_height -= 0.002;
    }

    let outer_radius = outer_diameter / 2.0;
    let inner_radius = inner_diameter / 2.0;

    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Helper to map Engine coordinates (x, y, z) to GLTF vertices based on SpaceView
    let map_vertex = |ex: f64, ey: f64, ez: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex {
                x: ex,
                y: ez,
                z: ey,
            }, // Swap Y and Z
            SpaceView::Vertical => Vertex {
                x: ex,
                y: ey,
                z: ez,
            }, // Direct mapping
        }
    };

    // Generate vertices for outer and inner cylinders
    let actual_segments = segments.max(3);

    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Outer Bottom
        vertices.push(map_vertex(
            cx + outer_radius * cos_a,
            cy + outer_radius * sin_a,
            cz,
        ));
        // Outer Top
        vertices.push(map_vertex(
            cx + outer_radius * cos_a,
            cy + outer_radius * sin_a,
            cz + actual_height,
        ));
        // Inner Bottom
        vertices.push(map_vertex(
            cx + inner_radius * cos_a,
            cy + inner_radius * sin_a,
            cz,
        ));
        // Inner Top
        vertices.push(map_vertex(
            cx + inner_radius * cos_a,
            cy + inner_radius * sin_a,
            cz + actual_height,
        ));
    }

    // Generate faces
    for i in 0..actual_segments as usize {
        let next = (i + 1) % actual_segments as usize;

        // Vertex indices for current and next segment
        let ob1 = i * 4;
        let ot1 = i * 4 + 1;
        let ib1 = i * 4 + 2;
        let it1 = i * 4 + 3;

        let ob2 = next * 4;
        let ot2 = next * 4 + 1;
        let ib2 = next * 4 + 2;
        let it2 = next * 4 + 3;

        // Outer side face (facing OUTWARDS)
        // Correct CCW winding: ob1 -> ob2 -> ot2 -> ot1
        faces.push(Face {
            vertices: vec![ob1, ob2, ot2, ot1],
        });

        // Inner side face (facing INWARDS)
        // Correct CCW winding: ib1 -> it1 -> it2 -> ib2
        faces.push(Face {
            vertices: vec![ib1, it1, it2, ib2],
        });

        if caps {
            // Bottom cap (ring) (facing DOWN)
            // Correct CCW winding: ob1 -> ib1 -> ib2 -> ob2
            faces.push(Face {
                vertices: vec![ob1, ib1, ib2, ob2],
            });

            // Top cap (ring) (facing UP)
            // Correct CCW winding: ot1 -> ot2 -> it2 -> it1
            faces.push(Face {
                vertices: vec![ot1, ot2, it2, it1],
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
