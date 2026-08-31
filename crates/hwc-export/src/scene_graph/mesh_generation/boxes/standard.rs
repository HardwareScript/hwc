use crate::scene_graph::types::{BoxParams, Face, FaceCulling, MeshNode, Vertex};
use hwc_engine::SpaceView;

/// Create a standard box mesh for components and other primitives.
pub fn create_box_mesh(
    name: &str,
    params: BoxParams,
    material_name: &str,
    view: SpaceView,
    culling: FaceCulling,
) -> MeshNode {
    let vertices = match view {
        SpaceView::Horizontal => {
            vec![
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
            vec![
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

    if !culling.bottom {
        faces.push(Face {
            vertices: vec![0, 3, 2, 1],
        });
    }
    if !culling.top {
        faces.push(Face {
            vertices: vec![4, 5, 6, 7],
        });
    }
    if !culling.front {
        faces.push(Face {
            vertices: vec![0, 1, 5, 4],
        });
    }
    if !culling.back {
        faces.push(Face {
            vertices: vec![2, 3, 7, 6],
        });
    }
    if !culling.left {
        faces.push(Face {
            vertices: vec![0, 4, 7, 3],
        });
    }
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
            vec![
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
            vec![
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
        },
        Face {
            vertices: vec![4, 5, 6, 7],
        },
        Face {
            vertices: vec![0, 1, 5, 4],
        },
        Face {
            vertices: vec![2, 3, 7, 6],
        },
        Face {
            vertices: vec![0, 4, 7, 3],
        },
        Face {
            vertices: vec![1, 2, 6, 5],
        },
    ];

    Ok(MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: material_name.into(),
    })
}
