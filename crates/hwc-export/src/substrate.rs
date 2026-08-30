//! 3D Substrate Triangulation Subsystem
//!
//! Implements Earcut polygon triangulation for 3D substrate geometry, dielectric cavities,
//! and GLB visual meshes.

use earcut::Earcut;

/// Represents a 3D vertex in nanometer or millimeter coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubstrateVertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Represents a triangular face with 3 vertex indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubstrateTriangle {
    pub v0: usize,
    pub v1: usize,
    pub v2: usize,
}

/// Triangulated 3D mesh representation of a substrate layer.
#[derive(Debug, Clone, Default)]
pub struct SubstrateMesh {
    pub name: String,
    pub vertices: Vec<SubstrateVertex>,
    pub triangles: Vec<SubstrateTriangle>,
}

/// Triangulates a 2D planar polygon contour with optional holes and extrudes it into a 3D solid.
pub fn triangulate_and_extrude(
    name: &str,
    outer_contour: &[(f64, f64)],
    holes: &[Vec<(f64, f64)>],
    z_min: f64,
    thickness: f64,
) -> SubstrateMesh {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut triangulator = Earcut::new();

    let mut flat_coords = Vec::new();
    let mut hole_indices = Vec::new();

    // Flatten outer boundary
    for &(x, y) in outer_contour {
        flat_coords.push(x);
        flat_coords.push(y);
    }

    // Flatten holes and note indices
    for hole in holes {
        hole_indices.push(flat_coords.len() / 2);
        for &(x, y) in hole {
            flat_coords.push(x);
            flat_coords.push(y);
        }
    }

    let vertex_count_2d = flat_coords.len() / 2;
    if vertex_count_2d < 3 {
        return SubstrateMesh {
            name: name.to_string(),
            vertices,
            triangles,
        };
    }

    // Generate 3D bottom and top vertices
    for i in 0..vertex_count_2d {
        let x = flat_coords[i * 2];
        let y = flat_coords[i * 2 + 1];

        // Bottom vertex (index: 2 * i)
        vertices.push(SubstrateVertex { x, y, z: z_min });
        // Top vertex (index: 2 * i + 1)
        vertices.push(SubstrateVertex {
            x,
            y,
            z: z_min + thickness,
        });
    }

    // Triangulate top/bottom caps using Earcut
    let mut earcut_triangles = Vec::new();
    let data = flat_coords.chunks_exact(2).map(|c| [c[0], c[1]]);
    triangulator.earcut(data, &hole_indices, &mut earcut_triangles);

    for chunk in earcut_triangles.chunks_exact(3) {
        let (i0, i1, i2) = (chunk[0], chunk[1], chunk[2]);

        // Bottom cap (facing -Z, CW winding)
        triangles.push(SubstrateTriangle {
            v0: i0 * 2,
            v1: i2 * 2,
            v2: i1 * 2,
        });

        // Top cap (facing +Z, CCW winding)
        triangles.push(SubstrateTriangle {
            v0: i0 * 2 + 1,
            v1: i1 * 2 + 1,
            v2: i2 * 2 + 1,
        });
    }

    // Generate side walls
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

            // Two triangles for each quad side wall
            triangles.push(SubstrateTriangle {
                v0: b_curr,
                v1: b_next,
                v2: t_next,
            });
            triangles.push(SubstrateTriangle {
                v0: b_curr,
                v1: t_next,
                v2: t_curr,
            });
        }
    }

    SubstrateMesh {
        name: name.to_string(),
        vertices,
        triangles,
    }
}
