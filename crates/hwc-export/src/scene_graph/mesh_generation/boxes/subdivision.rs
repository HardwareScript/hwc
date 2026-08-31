use crate::scene_graph::types::{BoxParams, MeshNode};
use crate::scene_graph::mesh_generation::boxes::{self, params, utils};
use crate::scene_graph::mesh_generation::boxes::params::CutoutParams;
use hwc_engine::SpaceView;

pub(super) fn subdivide_rect(root_mesh: &mut MeshNode, params: params::SubdivideRectParams) {
    let params::SubdivideRectParams {
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
    if (x2 - x1).abs() < 1e-4 || (y2 - y1).abs() < 1e-4 {
        return;
    }

    if depth < 1e-7 {
        return;
    }

    let map_vertex = |ex: f64, ey: f64, ez: f64| -> crate::scene_graph::types::Vertex {
        match view {
            SpaceView::Horizontal => crate::scene_graph::types::Vertex {
                x: ex,
                y: ez,
                z: ey,
            },
            SpaceView::Vertical => crate::scene_graph::types::Vertex {
                x: ex,
                y: ey,
                z: ez,
            },
        }
    };

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
        let sub_mesh = boxes::standard::create_box_mesh("zone", sub_params, &material_name, view, culling);
        utils::add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

    if local_cutouts.is_empty() {
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = boxes::standard::create_box_mesh("zone", sub_params, &material_name, view, base_culling);
        utils::add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

    let (hx1, hx2, hy1, hy2, is_cylinder, cylinder_params) = match local_cutouts[0] {
        CutoutParams::Cylinder {
            cx,
            cy,
            dia,
            z_min: _cz_min,
            z_max: _cz_max,
        } => {
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
        let sub_params = BoxParams::new(x1, y1, z_min, x2 - x1, y2 - y1, depth);
        let sub_mesh = boxes::standard::create_box_mesh("zone", sub_params, &material_name, view, base_culling);
        utils::add_to_mesh(root_mesh, sub_mesh.vertices, sub_mesh.faces);
        return;
    }

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
                    crate::scene_graph::mesh_generation::boxes::hole_zone::render_hole_zone(
                        root_mesh,
                        boxes::params::HoleZoneParams {
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
                }
            } else {
                subdivide_rect(
                    root_mesh,
                    params::SubdivideRectParams {
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
