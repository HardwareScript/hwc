use crate::scene_graph::types::{Face, MeshNode, Vertex};

pub(super) fn add_to_mesh(root: &mut MeshNode, sub_verts: Vec<Vertex>, sub_faces: Vec<Face>) {
    let offset = root.vertices.len();
    root.vertices.extend(sub_verts);
    for face in sub_faces {
        root.faces.push(Face {
            vertices: face.vertices.iter().map(|v| v + offset).collect(),
        });
    }
}
