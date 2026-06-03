//! Geometry generation utilities for meshes

use crate::scene_graph::types::{BoxParams, Face, FaceCulling, MeshNode, Vertex};
use hwc_engine::{SpaceView, voxel_grid::CapType};

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
                Vertex { x: params.x, y: params.z, z: params.y },
                Vertex { x: params.x + params.width, y: params.z, z: params.y },
                Vertex { x: params.x + params.width, y: params.z, z: params.y + params.height },
                Vertex { x: params.x, y: params.z, z: params.y + params.height },
                // Top face (Z-max)
                Vertex { x: params.x, y: params.z + params.depth, z: params.y },
                Vertex { x: params.x + params.width, y: params.z + params.depth, z: params.y },
                Vertex { x: params.x + params.width, y: params.z + params.depth, z: params.y + params.height },
                Vertex { x: params.x, y: params.z + params.depth, z: params.y + params.height },
            ]
        }
        SpaceView::Vertical => {
            // Vertical Standing (Y is Up)
            // Engine X -> GLTF X
            // Engine Y -> GLTF Y
            // Engine Z -> GLTF Z
            vec![
                // Bottom face (Z-min)
                Vertex { x: params.x, y: params.y, z: params.z },
                Vertex { x: params.x + params.width, y: params.y, z: params.z },
                Vertex { x: params.x + params.width, y: params.y + params.height, z: params.z },
                Vertex { x: params.x, y: params.y + params.height, z: params.z },
                // Top face (Z-max)
                Vertex { x: params.x, y: params.y, z: params.z + params.depth },
                Vertex { x: params.x + params.width, y: params.y, z: params.z + params.depth },
                Vertex { x: params.x + params.width, y: params.y + params.height, z: params.z + params.depth },
                Vertex { x: params.x, y: params.y + params.height, z: params.z + params.depth },
            ]
        }
    };

    let mut faces = Vec::new();

    // v0.1.7: Correct Winding Order (CCW from outside)
    // Bottom: 0-3-2-1 (Looking from below)
    if !culling.bottom {
        faces.push(Face { vertices: vec![0, 3, 2, 1] });
    }
    // Top: 4-5-6-7 (Looking from above)
    if !culling.top {
        faces.push(Face { vertices: vec![4, 5, 6, 7] });
    }
    // Front: 0-1-5-4 (Looking from front)
    if !culling.front {
        faces.push(Face { vertices: vec![0, 1, 5, 4] });
    }
    // Back: 2-3-7-6 (Looking from back)
    if !culling.back {
        faces.push(Face { vertices: vec![2, 3, 7, 6] });
    }
    // Left: 0-4-7-3 (Looking from left)
    if !culling.left {
        faces.push(Face { vertices: vec![0, 4, 7, 3] });
    }
    // Right: 1-2-6-5 (Looking from right)
    if !culling.right {
        faces.push(Face { vertices: vec![1, 2, 6, 5] });
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
) -> Result<MeshNode, super::materials::SceneGraphError> {
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
                Vertex { x: x - half_w, y: z - half_d, z: y - half_h },
                Vertex { x: x + half_w, y: z - half_d, z: y - half_h },
                Vertex { x: x + half_w, y: z - half_d, z: y + half_h },
                Vertex { x: x - half_w, y: z - half_d, z: y + half_h },
                // Top face
                Vertex { x: x - half_w, y: z + half_d, z: y - half_h },
                Vertex { x: x + half_w, y: z + half_d, z: y - half_h },
                Vertex { x: x + half_w, y: z + half_d, z: y + half_h },
                Vertex { x: x - half_w, y: z + half_d, z: y + half_h },
            ]
        }
        SpaceView::Vertical => {
            // Vertical Standing (Y is Up)
            vec![
                // Bottom face
                Vertex { x: x - half_w, y: y - half_h, z: z - half_d },
                Vertex { x: x + half_w, y: y - half_h, z: z - half_d },
                Vertex { x: x + half_w, y: y + half_h, z: z - half_d },
                Vertex { x: x - half_w, y: y + half_h, z: z - half_d },
                // Top face
                Vertex { x: x - half_w, y: y - half_h, z: z + half_d },
                Vertex { x: x + half_w, y: y - half_h, z: z + half_d },
                Vertex { x: x + half_w, y: y + half_h, z: z + half_d },
                Vertex { x: x - half_w, y: y + half_h, z: z + half_d },
            ]
        }
    };

    let faces = vec![
        Face { vertices: vec![0, 3, 2, 1] }, // Bottom (CCW from outside)
        Face { vertices: vec![4, 5, 6, 7] }, // Top
        Face { vertices: vec![0, 1, 5, 4] }, // Front
        Face { vertices: vec![2, 3, 7, 6] }, // Back
        Face { vertices: vec![0, 4, 7, 3] }, // Left
        Face { vertices: vec![1, 2, 6, 5] }, // Right
    ];

    Ok(MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    })
}

/// Create a cylindrical mesh (v0.1.6)
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

    // v0.1.7: Unified map_vertex for consistency across all mesh types
    let map_vertex = |ex: f64, ey: f64, ez: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex { x: ex, y: ez, z: ey },
            SpaceView::Vertical => Vertex { x: ex, y: ey, z: ez },
        }
    };

    // Generate vertices for top and bottom caps
    // Using 64 segments by default for "Perfect Circle" look if segments is 16
    let actual_segments = if segments == 16 { 64 } else { segments };

    for i in 0..actual_segments {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let dx = radius * angle.cos();
        let dy = radius * angle.sin();

        // Bottom vertex
        vertices.push(map_vertex(cx + dx, cy + dy, cz));
        // Top vertex
        vertices.push(map_vertex(cx + dx, cy + dy, cz + height));
    }

    // Generate side faces
    for i in 0..actual_segments as usize {
        let next = (i + 1) % actual_segments as usize;
        let b1 = i * 2;
        let t1 = i * 2 + 1;
        let b2 = next * 2;
        let t2 = next * 2 + 1;

        faces.push(Face {
            vertices: vec![b1, b2, t2, t1],
        });
    }

    // Generate top and bottom caps
    if !culling.bottom {
        let mut bottom_cap = Vec::new();
        for i in 0..actual_segments {
            bottom_cap.push((i * 2) as usize);
        }
        bottom_cap.reverse(); // Reverse for correct winding order
        faces.push(Face {
            vertices: bottom_cap,
        });
    }

    if !culling.top {
        let mut top_cap = Vec::new();
        for i in 0..actual_segments {
            top_cap.push((i * 2 + 1) as usize);
        }
        faces.push(Face { vertices: top_cap });
    }

    MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    }
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
        params.x,
        params.y,
        params.x + params.width,
        params.y + params.height,
        params.z,
        params.depth,
        &cutouts,
        material_name,
        view,
        culling,
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

fn subdivide_rect(
    root_mesh: &mut MeshNode,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    z_min: f64,
    depth: f64,
    cutouts: &[CutoutParams],
    material_name: &str,
    view: SpaceView,
    base_culling: FaceCulling,
) {
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
            SpaceView::Horizontal => Vertex { x: ex, y: ez, z: ey },
            SpaceView::Vertical => Vertex { x: ex, y: ey, z: ez },
        }
    };

    // Filter cutouts that intersect this rectangle
    let local_cutouts: Vec<_> = cutouts
        .iter()
        .copied()
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
            } => !(*rx1 >= x2 - 1e-7 || *rx2 <= x1 + 1e-7 || *ry1 >= y2 - 1e-7 || *ry2 <= y1 + 1e-7),
        })
        .collect();

    if local_cutouts.is_empty() {
        let mut culling = base_culling;
        // Check for surface-touching cutouts that cover this entire region
        for cutout in cutouts {
            match cutout {
                CutoutParams::Rect {
                    x1: rx1,
                    y1: ry1,
                    x2: rx2,
                    y2: ry2,
                    z_min: rz_min,
                    z_max: rz_max,
                } => {
                    if *rx1 <= x1 + 1e-7 && *rx2 >= x2 - 1e-7 && *ry1 <= y1 + 1e-7 && *ry2 >= y2 - 1e-7 {
                        // This cutout covers the entire region XY-wise
                        // Check for Z-surface contact (Manifold Rule)
                        if (*rz_min - (z_min + depth)).abs() < 1e-6 {
                            culling.top = true;
                        }
                        if (*rz_max - z_min).abs() < 1e-6 {
                            culling.bottom = true;
                        }
                    }
                }
                _ => {}
            }
        }

        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = create_box_mesh("zone", sub_params, material_name, view, culling);
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
        } => (rx1.max(x1), rx2.min(x2), ry1.max(y1), ry2.min(y2), false, None),
    };

    if hx1 >= hx2 - 1e-6 || hy1 >= hy2 - 1e-6 {
        // Cutout doesn't effectively intersect this region anymore
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = create_box_mesh("zone", sub_params, material_name, view, base_culling);
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
                        cx1,
                        cy1,
                        cx2,
                        cy2,
                        z_min,
                        depth,
                        hx,
                        hy,
                        hr,
                        &map_vertex,
                    );
                } else {
                    // Rectangular hole: render nothing here (punched out)
                }
            } else {
                // Recursively subdivide this sub-region
                subdivide_rect(
                    root_mesh,
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    z_min,
                    depth,
                    &local_cutouts,
                    material_name,
                    view,
                    base_culling,
                );
            }
        }
    }
}

fn render_hole_zone(
    root: &mut MeshNode,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    z_min: f64,
    depth: f64,
    hx: f64,
    hy: f64,
    hr: f64,
    map_vertex: &impl Fn(f64, f64, f64) -> Vertex,
) {
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
            let corner_idx = if angle >= 0.0 && angle < std::f64::consts::PI * 0.5 {
                base + 2
            } else if angle >= std::f64::consts::PI * 0.5 && angle < std::f64::consts::PI {
                base + 3
            } else if angle >= std::f64::consts::PI && angle < std::f64::consts::PI * 1.5 {
                base + 0
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
            let c_idx = base + match q {
                0 => 1,
                1 => 2,
                2 => 3,
                3 => 0,
                _ => 0,
            };
            let next_c_idx = base + match q {
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

/// Create a unified plated-through-hole mesh (Limitation 7 Fix)
/// This creates a single mesh containing the inner tube and the top/bottom pads.
pub fn create_via_mesh(
    name: &str,
    center: (f64, f64, f64),
    drill_dia: f64,
    pad_dia: f64,
    plating_thickness: f64,
    height: f64,
    segments: u32,
    top_cap: CapType,
    bottom_cap: CapType,
    bottom_drill_dia: Option<f64>, // NEW v0.1.7: Tapered Microvia support
    material_name: &str,
    view: SpaceView,
) -> MeshNode {
    let (cx, cy, cz) = center;
    let actual_height = height;

    let r_top_plating = drill_dia / 2.0;
    let r_top_inner = r_top_plating - plating_thickness;
    let r_pad = pad_dia / 2.0;

    let r_bottom_plating = bottom_drill_dia.unwrap_or(drill_dia) / 2.0;
    let r_bottom_inner = r_bottom_plating - plating_thickness;

    let actual_segments = if segments == 16 { 64 } else { segments };

    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    let map_vertex = |ex: f64, ey: f64, ez: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex { x: ex, y: ez, z: ey },
            SpaceView::Vertical => Vertex { x: ex, y: ey, z: ez },
        }
    };

    // Add vertices for bottom (z=0)
    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        vertices.push(map_vertex(cx + r_bottom_inner * cos_a, cy + r_bottom_inner * sin_a, cz));   // 0: Inner Bottom
        vertices.push(map_vertex(cx + r_bottom_plating * cos_a, cy + r_bottom_plating * sin_a, cz)); // 1: Plating Bottom
        vertices.push(map_vertex(cx + r_pad * cos_a, cy + r_pad * sin_a, cz));     // 2: Pad Bottom
    }

    // Add vertices for top (z=height)
    let top_offset = vertices.len();
    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        vertices.push(map_vertex(cx + r_top_inner * cos_a, cy + r_top_inner * sin_a, cz + actual_height));   // 3: Inner Top
        vertices.push(map_vertex(cx + r_top_plating * cos_a, cy + r_top_plating * sin_a, cz + actual_height)); // 4: Plating Top
        vertices.push(map_vertex(cx + r_pad * cos_a, cy + r_pad * sin_a, cz + actual_height));     // 5: Pad Top
    }

    // Add center vertices for solid caps
    let bottom_center_idx = vertices.len();
    vertices.push(map_vertex(cx, cy, cz));
    let top_center_idx = vertices.len();
    vertices.push(map_vertex(cx, cy, cz + actual_height));

    // Generate faces
    for i in 0..actual_segments as usize {
        let next = (i + 1) % actual_segments as usize;
        
        // Bottom indices (i*3)
        let bi_inner = i * 3;
        let bi_plat  = i * 3 + 1;
        let bi_pad   = i * 3 + 2;
        
        let bnext_inner = next * 3;
        let bnext_plat  = next * 3 + 1;
        let bnext_pad   = next * 3 + 2;

        // Top indices (top_offset + i*3)
        let ti_inner = top_offset + bi_inner;
        let ti_plat  = top_offset + bi_plat;
        let ti_pad   = top_offset + bi_pad;
        
        let tnext_inner = top_offset + bnext_inner;
        let tnext_plat  = top_offset + bnext_plat;
        let tnext_pad   = top_offset + bnext_pad;

        // 1. Inner Wall (facing INWARDS)
        // Correct CCW winding for inner tube: bi_inner -> ti_inner -> tnext_inner -> bnext_inner
        faces.push(Face { vertices: vec![bi_inner, ti_inner, tnext_inner, bnext_inner] });

        // 2. Plating Wall (facing OUTWARDS)
        // Correct CCW winding for outer tube: bi_plat -> bnext_plat -> tnext_plat -> ti_plat
        faces.push(Face { vertices: vec![bi_plat, bnext_plat, tnext_plat, ti_plat] });
        
        // 3. Bottom Cap
        match bottom_cap {
            CapType::Annular => {
                // Ring with hole: bi_pad -> bi_inner -> bnext_inner -> bnext_pad
                faces.push(Face {
                    vertices: vec![bi_pad, bi_inner, bnext_inner, bnext_pad],
                });
            }
            CapType::Solid => {
                // Solid disk: bottom_center -> bi_pad -> bnext_pad
                // Correct CCW winding for bottom face: center -> current -> next
                faces.push(Face {
                    vertices: vec![bottom_center_idx, bi_pad, bnext_pad],
                });
            }
            CapType::None => {}
        }

        // 4. Top Cap
        match top_cap {
            CapType::Annular => {
                // Ring with hole: ti_inner -> ti_pad -> tnext_pad -> tnext_inner
                faces.push(Face {
                    vertices: vec![ti_inner, ti_pad, tnext_pad, tnext_inner],
                });
            }
            CapType::Solid => {
                // Solid disk: top_center -> ti_pad -> tnext_pad
                // Correct CCW winding for top face: center -> current -> next
                faces.push(Face {
                    vertices: vec![top_center_idx, ti_pad, tnext_pad],
                });
            }
            CapType::None => {}
        }

        // NOTE: We removed the "Pad Side Wall" (Face 5) that connected top and bottom rings.
        // This ensures the cylinder remains a cylinder and rings remain flat disks.
    }

    MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    }
}

/// Create a tube (hollow cylinder) mesh (v0.1.7 Limitation 7)
pub fn create_tube_mesh(
    name: &str,
    center: (f64, f64, f64),
    outer_diameter: f64,
    inner_diameter: f64,
    height: f64,
    segments: u32,
    caps: bool, // v0.1.7: Option to enable/disable top/bottom rings
    material_name: &str,
    view: SpaceView,
) -> MeshNode {
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
            SpaceView::Horizontal => Vertex { x: ex, y: ez, z: ey }, // Swap Y and Z
            SpaceView::Vertical => Vertex { x: ex, y: ey, z: ez },   // Direct mapping
        }
    };

    // Generate vertices for outer and inner cylinders
    let actual_segments = if segments == 16 { 64 } else { segments };

    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Outer Bottom
        vertices.push(map_vertex(cx + outer_radius * cos_a, cy + outer_radius * sin_a, cz));
        // Outer Top
        vertices.push(map_vertex(cx + outer_radius * cos_a, cy + outer_radius * sin_a, cz + actual_height));
        // Inner Bottom
        vertices.push(map_vertex(cx + inner_radius * cos_a, cy + inner_radius * sin_a, cz));
        // Inner Top
        vertices.push(map_vertex(cx + inner_radius * cos_a, cy + inner_radius * sin_a, cz + actual_height));
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
