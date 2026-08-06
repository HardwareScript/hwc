use crate::scene_graph::types::{BoxParams, Face, FaceCulling, MeshNode, Vertex};
use hwc_engine::SpaceView;

/// Parameters for [`subdivide_rect`].
struct SubdivideRectParams {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    z_min: f64,
    depth: f64,
    cutouts: Vec<CutoutParams>,
    material_name: String,
    view: SpaceView,
    base_culling: FaceCulling,
}

/// Parameters for [`render_hole_zone`].
struct HoleZoneParams<'a> {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    z_min: f64,
    depth: f64,
    hx: f64,
    hy: f64,
    hr: f64,
    map_vertex: &'a dyn Fn(f64, f64, f64) -> Vertex,
}

/// Create a standard box mesh for components and other primitives.
pub fn create_box_mesh(
    name: &str,
    params: BoxParams,
    material_name: &str,
    view: SpaceView,
    culling: FaceCulling,
) -> MeshNode {
    // Determine GLTF axes based on SpaceView orientation
    // Standard GLTF: X: Right, Y: Up, Z: Forward

    // Engine Standard: X: Width, Y: Height, Z: Depth (Layers)

    let vertices = match view {
        SpaceView::Horizontal => {
            // Horizontal Floor (Z is Up)
            // Engine X -> GLTF X
            // Engine Y -> GLTF Z
            // Engine Z -> GLTF Y
            vec![
                // Bottom face (Z-min)
                Vertex {
                    x: params.x,
                    y: params.z,
                    z: params.y,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.z,
                    z: params.y,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.z,
                    z: params.y + params.height,
                },
                Vertex {
                    x: params.x,
                    y: params.z,
                    z: params.y + params.height,
                },
                // Top face (Z-max)
                Vertex {
                    x: params.x,
                    y: params.z + params.depth,
                    z: params.y,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.z + params.depth,
                    z: params.y,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.z + params.depth,
                    z: params.y + params.height,
                },
                Vertex {
                    x: params.x,
                    y: params.z + params.depth,
                    z: params.y + params.height,
                },
            ]
        }
        SpaceView::Vertical => {
            // Vertical Standing (Y is Up)
            // Engine X -> GLTF X
            // Engine Y -> GLTF Y
            // Engine Z -> GLTF Z
            vec![
                // Bottom face (Z-min)
                Vertex {
                    x: params.x,
                    y: params.y,
                    z: params.z,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.y,
                    z: params.z,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.y + params.height,
                    z: params.z,
                },
                Vertex {
                    x: params.x,
                    y: params.y + params.height,
                    z: params.z,
                },
                // Top face (Z-max)
                Vertex {
                    x: params.x,
                    y: params.y,
                    z: params.z + params.depth,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.y,
                    z: params.z + params.depth,
                },
                Vertex {
                    x: params.x + params.width,
                    y: params.y + params.height,
                    z: params.z + params.depth,
                },
                Vertex {
                    x: params.x,
                    y: params.y + params.height,
                    z: params.z + params.depth,
                },
            ]
        }
    };

    let mut faces = Vec::new();

    // v0.1.7: Correct Winding Order (CCW from outside)
    // Bottom: 0-3-2-1 (Looking from below)
    if !culling.bottom {
        faces.push(Face {
            vertices: vec![0, 3, 2, 1],
        });
    }
    // Top: 4-5-6-7 (Looking from above)
    if !culling.top {
        faces.push(Face {
            vertices: vec![4, 5, 6, 7],
        });
    }
    // Front: 0-1-5-4 (Looking from front)
    if !culling.front {
        faces.push(Face {
            vertices: vec![0, 1, 5, 4],
        });
    }
    // Back: 2-3-7-6 (Looking from back)
    if !culling.back {
        faces.push(Face {
            vertices: vec![2, 3, 7, 6],
        });
    }
    // Left: 0-4-7-3 (Looking from left)
    if !culling.left {
        faces.push(Face {
            vertices: vec![0, 4, 7, 3],
        });
    }
    // Right: 1-2-6-5 (Looking from right)
    if !culling.right {
        faces.push(Face {
            vertices: vec![1, 2, 6, 5],
        });
    }

    MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    }
}

/// Create a mesh for a component body with optional color.
pub fn create_component_box(
    name: &str,
    center: (f64, f64, f64),
    dims: (f64, f64, f64),
    material_name: &str,
    view: SpaceView,
) -> Result<MeshNode, crate::scene_graph::materials::SceneGraphError> {
    let (x, y, z) = center;
    let (width, height, depth) = dims;
    let half_w = width / 2.0;
    let half_h = height / 2.0;
    let half_d = depth / 2.0;

    let vertices = match view {
        SpaceView::Horizontal => {
            // Horizontal Floor (Z is Up)
            vec![
                // Bottom face
                Vertex {
                    x: x - half_w,
                    y: z - half_d,
                    z: y - half_h,
                },
                Vertex {
                    x: x + half_w,
                    y: z - half_d,
                    z: y - half_h,
                },
                Vertex {
                    x: x + half_w,
                    y: z - half_d,
                    z: y + half_h,
                },
                Vertex {
                    x: x - half_w,
                    y: z - half_d,
                    z: y + half_h,
                },
                // Top face
                Vertex {
                    x: x - half_w,
                    y: z + half_d,
                    z: y - half_h,
                },
                Vertex {
                    x: x + half_w,
                    y: z + half_d,
                    z: y - half_h,
                },
                Vertex {
                    x: x + half_w,
                    y: z + half_d,
                    z: y + half_h,
                },
                Vertex {
                    x: x - half_w,
                    y: z + half_d,
                    z: y + half_h,
                },
            ]
        }
        SpaceView::Vertical => {
            // Vertical Standing (Y is Up)
            vec![
                // Bottom face
                Vertex {
                    x: x - half_w,
                    y: y - half_h,
                    z: z - half_d,
                },
                Vertex {
                    x: x + half_w,
                    y: y - half_h,
                    z: z - half_d,
                },
                Vertex {
                    x: x + half_w,
                    y: y + half_h,
                    z: z - half_d,
                },
                Vertex {
                    x: x - half_w,
                    y: y + half_h,
                    z: z - half_d,
                },
                // Top face
                Vertex {
                    x: x - half_w,
                    y: y - half_h,
                    z: z + half_d,
                },
                Vertex {
                    x: x + half_w,
                    y: y - half_h,
                    z: z + half_d,
                },
                Vertex {
                    x: x + half_w,
                    y: y + half_h,
                    z: z + half_d,
                },
                Vertex {
                    x: x - half_w,
                    y: y + half_h,
                    z: z + half_d,
                },
            ]
        }
    };

    let faces = vec![
        Face {
            vertices: vec![0, 3, 2, 1],
        }, // Bottom (CCW from outside)
        Face {
            vertices: vec![4, 5, 6, 7],
        }, // Top
        Face {
            vertices: vec![0, 1, 5, 4],
        }, // Front
        Face {
            vertices: vec![2, 3, 7, 6],
        }, // Back
        Face {
            vertices: vec![0, 4, 7, 3],
        }, // Left
        Face {
            vertices: vec![1, 2, 6, 5],
        }, // Right
    ];

    Ok(MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    })
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
        return create_box_mesh(name, params, material_name, view, culling);
    }

    let mut root_mesh = MeshNode {
        name: name.into(),
        vertices: Vec::new(),
        faces: Vec::new(),
        material_name: material_name.into(),
    };

    subdivide_rect(
        &mut root_mesh,
        SubdivideRectParams {
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

fn add_to_mesh(root: &mut MeshNode, sub_verts: Vec<Vertex>, sub_faces: Vec<Face>) {
    let offset = root.vertices.len();
    root.vertices.extend(sub_verts);
    for face in sub_faces {
        root.faces.push(Face {
            vertices: face.vertices.iter().map(|v| v + offset).collect(),
        });
    }
}

fn subdivide_rect(root_mesh: &mut MeshNode, params: SubdivideRectParams) {
    let SubdivideRectParams {
        x1,
        y1,
        x2,
        y2,
        z_min,
        depth,
        cutouts,
        material_name,
        view,
        base_culling,
    } = params;

    let mut culling = base_culling;
    // v0.1.7: Epsilon Guard (Prevent sliver polygons and mesh tearing)
    // If a region is smaller than 100nm, we don't render it.
    if (x2 - x1).abs() < 1e-4 || (y2 - y1).abs() < 1e-4 {
        return;
    }

    // v0.1.7: Prevent negative or degenerate depth
    if depth < 1e-7 {
        return;
    }

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

    // Filter cutouts that intersect this rectangle
    let local_cutouts: Vec<_> = cutouts
        .iter()
        .filter(|c| match c {
            CutoutParams::Cylinder { cx, cy, .. } => {
                *cx >= x1 - 1e-7 && *cx <= x2 + 1e-7 && *cy >= y1 - 1e-7 && *cy <= y2 + 1e-7
            }
            CutoutParams::Rect {
                x1: rx1,
                y1: ry1,
                x2: rx2,
                y2: ry2,
                ..
            } => {
                !(*rx1 >= x2 - 1e-7 || *rx2 <= x1 + 1e-7 || *ry1 >= y2 - 1e-7 || *ry2 <= y1 + 1e-7)
            }
        })
        .copied()
        .collect();

    // Check for surface-touching cutouts that cover this entire region
    for cutout in cutouts {
        if let CutoutParams::Rect {
            x1: rx1,
            y1: ry1,
            x2: rx2,
            y2: ry2,
            z_min: rz_min,
            z_max: rz_max,
        } = cutout
        {
            if rx1 <= x1 + 1e-7 && rx2 >= x2 - 1e-7 && ry1 <= y1 + 1e-7 && ry2 >= y2 - 1e-7 {
                // This cutout covers the entire region XY-wise
                // Check for Z-surface contact (Manifold Rule)
                if (rz_min - (z_min + depth)).abs() < 1e-6 {
                    culling.top = true;
                }
                if (rz_max - z_min).abs() < 1e-6 {
                    culling.bottom = true;
                }
            }
        }
    }

    if culling.top && culling.bottom {
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = create_box_mesh("zone", sub_params, &material_name, view, culling);
        add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

    // **v0.2.2 FIX**: If no cutouts actually intersect this region, render as solid box
    if local_cutouts.is_empty() {
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = create_box_mesh("zone", sub_params, &material_name, view, base_culling);
        add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

    // Pick the first cutout to partition around
    let (hx1, hx2, hy1, hy2, is_cylinder, cylinder_params) = match local_cutouts[0] {
        CutoutParams::Cylinder {
            cx,
            cy,
            dia,
            z_min: _cz_min,
            z_max: _cz_max,
        } => {
            // Implement 1μm Epsilon Offset on the hole radius (radius + 1μm)
            let hr = dia / 2.0 + 0.001;
            (
                (cx - hr).max(x1),
                (cx + hr).min(x2),
                (cy - hr).max(y1),
                (cy + hr).min(y2),
                true,
                Some((cx, cy, hr)),
            )
        }
        CutoutParams::Rect {
            x1: rx1,
            y1: ry1,
            x2: rx2,
            y2: ry2,
            ..
        } => (
            rx1.max(x1),
            rx2.min(x2),
            ry1.max(y1),
            ry2.min(y2),
            false,
            None,
        ),
    };

    if hx1 >= hx2 - 1e-6 || hy1 >= hy2 - 1e-6 {
        // Cutout doesn't effectively intersect this region anymore
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = create_box_mesh("zone", sub_params, &material_name, view, base_culling);
        add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

    // Partition the current rectangle into up to 9 sub-rectangles
    let i_x = [(x1, hx1), (hx1, hx2), (hx2, x2)];
    let i_y = [(y1, hy1), (hy1, hy2), (hy2, y2)];

    for (idx_x, &(cx1, cx2)) in i_x.iter().enumerate() {
        for (idx_y, &(cy1, cy2)) in i_y.iter().enumerate() {
            if cx2 - cx1 < 1e-6 || cy2 - cy1 < 1e-6 {
                continue;
            }

            if idx_x == 1 && idx_y == 1 {
                if is_cylinder {
                    let (hx, hy, hr) = cylinder_params.unwrap();
                    // Hole zone: render the circle inside [cx1, cx2] x [cy1, cy2]
                    render_hole_zone(
                        root_mesh,
                        HoleZoneParams {
                            x1: cx1,
                            y1: cy1,
                            x2: cx2,
                            y2: cy2,
                            z_min,
                            depth,
                            hx,
                            hy,
                            hr,
                            map_vertex: &map_vertex,
                        },
                    );
                } else {
                    // Rectangular hole: render nothing here (punched out)
                }
            } else {
                // Recursively subdivide this sub-region
                subdivide_rect(
                    root_mesh,
                    SubdivideRectParams {
                        x1: cx1,
                        y1: cy1,
                        x2: cx2,
                        y2: cy2,
                        z_min,
                        depth,
                        cutouts: local_cutouts.to_vec(),
                        material_name: material_name.clone(),
                        view,
                        base_culling,
                    },
                );
            }
        }
    }
}

fn render_hole_zone(root: &mut MeshNode, params: HoleZoneParams) {
    let HoleZoneParams {
        x1,
        y1,
        x2,
        y2,
        z_min,
        depth,
        hx,
        hy,
        hr,
        map_vertex,
    } = params;
    let segments = 64usize;
    let mut hz_verts = Vec::new();
    let mut hz_faces = Vec::new();

    for z_offset in [0.0, depth] {
        let ez = z_min + z_offset;
        let base = hz_verts.len();

        // Add 4 corners of the square
        hz_verts.push(map_vertex(x1, y1, ez)); // 0: BL
        hz_verts.push(map_vertex(x2, y1, ez)); // 1: BR
        hz_verts.push(map_vertex(x2, y2, ez)); // 2: TR
        hz_verts.push(map_vertex(x1, y2, ez)); // 3: TL

        // Add circle points
        let circle_start = hz_verts.len();
        for s in 0..segments {
            let angle = (s as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            hz_verts.push(map_vertex(hx + angle.cos() * hr, hy + angle.sin() * hr, ez));
        }

        // Triangulate cap
        for s in 0..segments {
            let s1 = circle_start + s;
            let s2 = circle_start + (s + 1) % segments;
            let angle = (s as f64 / segments as f64) * 2.0 * std::f64::consts::PI;

            // Correct quadrant mapping:
            // Quadrant I (0 <= angle < PI/2): Top-Right -> TR (base + 2)
            // Quadrant II (PI/2 <= angle < PI): Top-Left -> TL (base + 3)
            // Quadrant III (PI <= angle < 1.5*PI): Bottom-Left -> BL (base + 0)
            // Quadrant IV (1.5*PI <= angle < 2*PI): Bottom-Right -> BR (base + 1)
            let corner_idx = if (0.0..std::f64::consts::PI * 0.5).contains(&angle) {
                base + 2
            } else if (std::f64::consts::PI * 0.5..std::f64::consts::PI).contains(&angle) {
                base + 3
            } else if (std::f64::consts::PI..std::f64::consts::PI * 1.5).contains(&angle) {
                base
            } else {
                base + 1
            };

            if z_offset > 0.0 {
                hz_faces.push(Face {
                    vertices: vec![s1, s2, corner_idx],
                });
            } else {
                hz_faces.push(Face {
                    vertices: vec![s1, corner_idx, s2],
                });
            }
        }

        // Corner triangles
        for q in 0..4 {
            let s_idx = circle_start + (q * segments / 4);
            let c_idx = base
                + match q {
                    0 => 1,
                    1 => 2,
                    2 => 3,
                    3 => 0,
                    _ => 0,
                };
            let next_c_idx = base
                + match q {
                    0 => 2,
                    1 => 3,
                    2 => 0,
                    3 => 1,
                    _ => 1,
                };
            if z_offset > 0.0 {
                hz_faces.push(Face {
                    vertices: vec![s_idx, c_idx, next_c_idx],
                });
            } else {
                hz_faces.push(Face {
                    vertices: vec![s_idx, next_c_idx, c_idx],
                });
            }
        }
    }

    // Inner wall
    let top_base = hz_verts.len() / 2;
    let circle_start = 4;
    for s in 0..segments {
        let b1 = circle_start + s;
        let b2 = circle_start + (s + 1) % segments;
        let t1 = top_base + b1;
        let t2 = top_base + b2;
        hz_faces.push(Face {
            vertices: vec![b1, t1, t2, b2],
        });
    }

    add_to_mesh(root, hz_verts, hz_faces);
}
