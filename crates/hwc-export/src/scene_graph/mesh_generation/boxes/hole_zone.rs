use crate::scene_graph::types::{Face, MeshNode};
use crate::scene_graph::mesh_generation::boxes::utils;

pub(super) fn render_hole_zone(root: &mut MeshNode, params: super::params::HoleZoneParams) {
    let super::params::HoleZoneParams {
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

        hz_verts.push(map_vertex(x1, y1, ez));
        hz_verts.push(map_vertex(x2, y1, ez));
        hz_verts.push(map_vertex(x2, y2, ez));
        hz_verts.push(map_vertex(x1, y2, ez));

        let circle_start = hz_verts.len();
        for s in 0..segments {
            let angle = (s as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            hz_verts.push(map_vertex(hx + angle.cos() * hr, hy + angle.sin() * hr, ez));
        }

        for s in 0..segments {
            let s1 = circle_start + s;
            let s2 = circle_start + (s + 1) % segments;
            let angle = (s as f64 / segments as f64) * 2.0 * std::f64::consts::PI;

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

    utils::add_to_mesh(root, hz_verts, hz_faces);
}
