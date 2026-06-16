//! Extruded ribbon mesh generation for traces and paths

use super::geometry::calculate_perpendicular;
use super::types::{Face, MeshNode, Vertex};
use hwc_engine::SpaceView;

/// Parameters for arc section generation
struct ArcParams {
    center: (f64, f64),
    start: f64,
    sweep: f64,
    segs: usize,
    r: f64,
    z: f64,
    h: f64,
}

/// Parameters for joint fillet generation
struct FilletParams {
    p: (f64, f64),
    c: (f64, f64),
    n: (f64, f64),
    hw: f64,
    z: f64,
    h: f64,
    segs: usize,
}

/// Add an extruded ribbon (3D prism) along a polyline path
pub fn create_extruded_ribbon(
    name: &str,
    path: &[(f64, f64)],
    width: f64,
    height: f64,
    z_base: f64,
    material_name: &str,
    view: SpaceView,
) -> Option<MeshNode> {
    if path.len() < 2 {
        return None;
    }

    let half_width = width / 2.0;
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let arc_res = 16;

    for i in 0..path.len() {
        if i == 0 {
            let (px, py) = calculate_perpendicular(path, 0);
            let start_angle = py.atan2(px);
            add_arc_sections(
                &mut vertices,
                ArcParams {
                    center: path[0],
                    start: start_angle,
                    sweep: std::f64::consts::PI,
                    segs: arc_res,
                    r: half_width,
                    z: z_base,
                    h: height,
                },
                view,
            );
        } else if i == path.len() - 1 {
            let (px, py) = calculate_perpendicular(path, i);
            let start_angle = py.atan2(px) + std::f64::consts::PI;
            add_arc_sections(
                &mut vertices,
                ArcParams {
                    center: path[i],
                    start: start_angle,
                    sweep: std::f64::consts::PI,
                    segs: arc_res,
                    r: half_width,
                    z: z_base,
                    h: height,
                },
                view,
            );
        } else {
            let prev = path[i - 1];
            let curr = path[i];
            let next = path[i + 1];
            let v1 = (curr.0 - prev.0, curr.1 - prev.1);
            let v2 = (next.0 - curr.0, next.1 - curr.1);
            let mag1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
            let mag2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();

            if mag1 > 1e-9 && mag2 > 1e-9 {
                let dot = (v1.0 * v2.0 + v1.1 * v2.1) / (mag1 * mag2);
                if dot < 0.999 {
                    add_joint_fillet(
                        &mut vertices,
                        FilletParams {
                            p: prev,
                            c: curr,
                            n: next,
                            hw: half_width,
                            z: z_base,
                            h: height,
                            segs: arc_res / 2,
                        },
                        view,
                    );
                } else {
                    let (px, py) = calculate_perpendicular(path, i);
                    add_cross_section(
                        &mut vertices,
                        path[i],
                        px,
                        py,
                        half_width,
                        z_base,
                        height,
                        view,
                    );
                }
            } else {
                let (px, py) = calculate_perpendicular(path, i);
                add_cross_section(
                    &mut vertices,
                    path[i],
                    px,
                    py,
                    half_width,
                    z_base,
                    height,
                    view,
                );
            }
        }
    }

    let num_sections = vertices.len() / 4;
    for i in 0..(num_sections - 1) {
        let b = i * 4;
        let n = (i + 1) * 4;
        faces.push(Face {
            vertices: vec![b, b + 2, n + 2],
        });
        faces.push(Face {
            vertices: vec![b, n + 2, n],
        });
        faces.push(Face {
            vertices: vec![b + 1, n + 1, n + 3],
        });
        faces.push(Face {
            vertices: vec![b + 1, n + 3, b + 3],
        });
        faces.push(Face {
            vertices: vec![b, n, n + 1],
        });
        faces.push(Face {
            vertices: vec![b, n + 1, b + 1],
        });
        faces.push(Face {
            vertices: vec![b + 2, b + 3, n + 3],
        });
        faces.push(Face {
            vertices: vec![b + 2, n + 3, n + 2],
        });
    }

    Some(MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    })
}

/// Helper: Adds a straight cross-section with axis-swapping support
#[allow(clippy::too_many_arguments)]
fn add_cross_section(
    verts: &mut Vec<Vertex>,
    p: (f64, f64),
    px: f64,
    py: f64,
    hw: f64,
    z: f64,
    h: f64,
    view: SpaceView,
) {
    let pts = [
        (p.0 + px * hw, p.1 + py * hw, z),     // Bottom Left
        (p.0 - px * hw, p.1 - py * hw, z),     // Bottom Right
        (p.0 + px * hw, p.1 + py * hw, z + h), // Top Left
        (p.0 - px * hw, p.1 - py * hw, z + h), // Top Right
    ];

    for (ex, ey, ez) in pts {
        match view {
            SpaceView::Horizontal => {
                // Engine X -> GLTF X, Engine Y -> GLTF Z, Engine Z -> GLTF Y
                verts.push(Vertex {
                    x: ex,
                    y: ez,
                    z: ey,
                });
            }
            SpaceView::Vertical => {
                verts.push(Vertex {
                    x: ex,
                    y: ey,
                    z: ez,
                });
            }
        }
    }
}

fn add_arc_sections(verts: &mut Vec<Vertex>, params: ArcParams, view: SpaceView) {
    for i in 0..=params.segs {
        let t = i as f64 / params.segs as f64;
        let angle = params.start + params.sweep * t;
        add_cross_section(
            verts,
            params.center,
            angle.cos(),
            angle.sin(),
            params.r,
            params.z,
            params.h,
            view,
        );
    }
}

fn add_joint_fillet(verts: &mut Vec<Vertex>, params: FilletParams, view: SpaceView) {
    let dx1 = params.c.0 - params.p.0;
    let dy1 = params.c.1 - params.p.1;
    let dx2 = params.n.0 - params.c.0;
    let dy2 = params.n.1 - params.c.1;
    let mag1 = (dx1 * dx1 + dy1 * dy1).sqrt();
    let mag2 = (dx2 * dx2 + dy2 * dy2).sqrt();

    if mag1 < 1e-9 || mag2 < 1e-9 {
        return;
    }

    let (px1, py1) = (-dy1 / mag1, dx1 / mag1);
    let (px2, py2) = (-dy2 / mag2, dx2 / mag2);
    let start = py1.atan2(px1);
    let end = py2.atan2(px2);
    let mut diff = end - start;
    if diff > std::f64::consts::PI {
        diff -= 2.0 * std::f64::consts::PI;
    }
    if diff < -std::f64::consts::PI {
        diff += 2.0 * std::f64::consts::PI;
    }

    for i in 0..=params.segs {
        let t = i as f64 / params.segs as f64;
        let a = start + diff * t;
        add_cross_section(
            verts,
            params.c,
            a.cos(),
            a.sin(),
            params.hw,
            params.z,
            params.h,
            view,
        );
    }
}
