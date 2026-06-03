use crate::scene_graph::types::{Face, MeshNode, Vertex};
use hwc_engine::space::SpaceView;
use earcut::Earcut;

/// Extrudes flat 2D contours (with potential holes) into a solid 3D mesh
pub fn extrude_polygon_mesh(
    name: &str,
    outer_contour: &[(f64, f64)],
    holes: &[Vec<(f64, f64)>],
    z_min: f64,
    depth: f64,
    material_name: &str,
    view: SpaceView,
) -> MeshNode {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut triangulator = Earcut::new();

    // Helper to map 2D coordinates to 3D scene space based on active view
    let map_vertex = |ex: f64, ey: f64, ez: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex { x: ex, y: ez, z: ey },
            SpaceView::Vertical => Vertex { x: ex, y: ey, z: ez },
        }
    };

    // Flatten vertices for earcut input: [x0,y0, x1,y1, ...]
    let mut flat_coords = Vec::new();
    let mut hole_indices = Vec::new();

    // Add outer contour
    for &(x, y) in outer_contour {
        flat_coords.push(x);
        flat_coords.push(y);
    }

    // Add holes and track their starting indices
    for hole in holes {
        hole_indices.push(flat_coords.len() / 2);
        for &(x, y) in hole {
            flat_coords.push(x);
            flat_coords.push(y);
        }
    }

    // Generate 3D Vertices for Bottom (z_min) and Top (z_min + depth)
    let vertex_count_2d = flat_coords.len() / 2;
    for i in 0..vertex_count_2d {
        let x = flat_coords[i * 2];
        let y = flat_coords[i * 2 + 1];

        vertices.push(map_vertex(x, y, z_min));
        vertices.push(map_vertex(x, y, z_min + depth));
    }

    // 1. Triangulate Caps using Earcut (GeoRust Optimized)
    let mut triangles = Vec::new();
    let data = flat_coords.chunks_exact(2).map(|c| [c[0], c[1]]);
    triangulator.earcut(data, &hole_indices, &mut triangles);
    
    for chunk in triangles.chunks_exact(3) {
        let (v0, v1, v2) = (chunk[0], chunk[1], chunk[2]);

        // Bottom Cap (facing down, CW winding from inside)
        faces.push(Face {
            vertices: vec![v0 * 2, v2 * 2, v1 * 2],
        });

        // Top Cap (facing up, CCW winding from outside)
        faces.push(Face {
            vertices: vec![v0 * 2 + 1, v1 * 2 + 1, v2 * 2 + 1],
        });
    }

    // 2. Generate side walls
    // We must generate wall segments for the outer boundary and each hole
    let mut contour_ranges = vec![0];
    contour_ranges.extend(hole_indices.iter().cloned());
    contour_ranges.push(vertex_count_2d);

    for r in 0..(contour_ranges.len() - 1) {
        let start = contour_ranges[r];
        let end = contour_ranges[r + 1];
        let count = end - start;

        for i in 0..count {
            let curr = start + i;
            let next = start + (i + 1) % count;

            let b_curr = curr * 2;
            let t_curr = curr * 2 + 1;
            let b_next = next * 2;
            let t_next = next * 2 + 1;

            // Quad face connecting bottom and top segments
            faces.push(Face {
                vertices: vec![b_curr, b_next, t_next, t_curr],
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
