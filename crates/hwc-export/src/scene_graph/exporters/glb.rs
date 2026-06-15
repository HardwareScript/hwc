//! GLB (glTF binary) export functionality

use crate::scene_graph::materials::get_or_create_material;
use crate::scene_graph::types::MaterialNode;
use crate::scene_graph::types::MeshNode;
use crate::scene_graph::types::Face; // FIXED: Imported Face type for batching
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde_json::json;

/// Export scene graph to GLB format (glTF binary) with proper per-material meshes
pub fn export_glb(
    materials: &FxHashMap<CompactString, MaterialNode>,
    meshes: &[MeshNode],
) -> Vec<u8> {
    // =========================================================================
    // OPTIMIZATION: BATCH OVERLAPPING SEGMENTS BY NAME & MATERIAL (v0.1.7)
    // =========================================================================
    let mut batched_map: FxHashMap<(CompactString, CompactString), MeshNode> = FxHashMap::default();

    for mesh in meshes {
        // Group by both material and name to keep nets together while preserving independent component pads
        let key = (mesh.material_name.clone(), mesh.name.clone());

        let group = batched_map.entry(key).or_insert_with(|| MeshNode {
            name: mesh.name.clone(),
            vertices: Vec::new(),
            faces: Vec::new(),
            material_name: mesh.material_name.clone(),
        });

        let vertex_offset = group.vertices.len();
        group.vertices.extend(&mesh.vertices);

        for face in &mesh.faces {
            let offset_vertices: Vec<usize> = face.vertices.iter()
                .map(|&idx| idx + vertex_offset)
                .collect();
            
            group.faces.push(Face { vertices: offset_vertices });
        }
    }

    // Use the optimized meshes vector instead of the fragmented raw list
    // This reduces our 128-bit test from 3,328 meshes down to ~160 meshes
    let optimized_meshes: Vec<MeshNode> = batched_map.into_values().collect();

    let mut mat_map: FxHashMap<CompactString, usize> = FxHashMap::default();
    let mut materials_array = Vec::new();

    // 1. Build Material Palette with depth bias metadata
    for (i, (name, mat)) in materials.iter().enumerate() {
        mat_map.insert(name.clone(), i);
        let (r, g, b) = mat.color.to_normalized();
        
        let is_transparent = mat.opacity < 1.0;
        let has_jelly_effect = mat.subsurface > 0.0;

        // v0.1.7: Decoupled Transparency (Opacity) from Optics (Subsurface)
        // 1. Standard Alpha Blending (Smooth transparency)
        let alpha_mode = if is_transparent { "BLEND" } else { "OPAQUE" };

        let pbr = json!({
            "baseColorFactor": [r, g, b, mat.opacity],
            "metallicFactor": mat.metallic,
            "roughnessFactor": mat.roughness
        });

        let mut material_json = json!({
            "name": name,
            "pbrMetallicRoughness": pbr,
            "alphaMode": alpha_mode,
            "doubleSided": true
        });

        // v0.1.7: Zero-Flicker GPU Handshake (Depth Bias)
        // Conductors and high-precedence materials get a priority offset to resolve depth ties
        if mat.precedence < 4 {
            let factor = (mat.precedence as f32) - 4.0; // 1 -> -3.0, 2 -> -2.0, 3 -> -1.0
            material_json["extras"] = json!({
                "polygonOffset": true,
                "polygonOffsetFactor": factor,
                "polygonOffsetUnits": factor,
                "renderOrder": (10 - mat.precedence) as i32
            });
        }

        // Add High-Fidelity Optics Extensions (v0.1.7)
        let mut extensions = json!({});

        // 1. KHR_materials_ior (Refraction)
        if (mat.ior - 1.5).abs() > 0.001 || has_jelly_effect {
            extensions["KHR_materials_ior"] = json!({
                "ior": if has_jelly_effect && mat.ior == 1.5 { 1.2 } else { mat.ior }
            });
        }

        // 2. KHR_materials_clearcoat (The glossy surface shine)
        if mat.clearcoat > 0.0 || has_jelly_effect {
            let clearcoat_factor = if has_jelly_effect && mat.clearcoat == 0.0 { 0.5 } else { mat.clearcoat };
            extensions["KHR_materials_clearcoat"] = json!({
                "clearcoatFactor": clearcoat_factor,
                "clearcoatRoughnessFactor": mat.clearcoat_roughness
            });
        }

        // 3. KHR_materials_transmission (Internal light pass-through)
        if has_jelly_effect {
            extensions["KHR_materials_transmission"] = json!({
                "transmissionFactor": mat.subsurface 
            });

            // 4. KHR_materials_volume (The "Jelly" depth effect)
            extensions["KHR_materials_volume"] = json!({
                "thicknessFactor": 0.001, // 1mm thickness
                "attenuationColor": [r, g, b],
                "attenuationDistance": 0.5
            });
        }

        // 5. KHR_materials_anisotropy (v0.1.7 - For surface grain/weave)
        if mat.anisotropy > 0.0 {
            extensions["KHR_materials_anisotropy"] = json!({
                "anisotropyStrength": mat.anisotropy,
                "anisotropyRotation": mat.anisotropy_rotation
            });
        }

        if !extensions.as_object().unwrap().is_empty() {
            material_json["extensions"] = extensions;
        }

        materials_array.push(material_json);
    }

    // 1.5. Collect unknown materials from meshes and add them dynamically using lookup table
    let mut materials_owned = materials.clone();
    for mesh in &optimized_meshes { // FIXED: Iterate over optimized_meshes
        if !mat_map.contains_key(&mesh.material_name) {
            let mat_idx = materials_array.len();
            let (material_node, _) = get_or_create_material(&mut materials_owned, &mesh.material_name);
            mat_map.insert(mesh.material_name.clone(), mat_idx);

            // Add fallback material with inferred properties
            let (r, g, b) = material_node.color.to_normalized();
            let precedence = material_node.precedence;
            let factor = if precedence < 4 {
                Some((precedence as f32) - 4.0)
            } else {
                None
            };

            let mut material_json = json!({
                "name": mesh.material_name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [r, g, b, material_node.opacity],
                    "metallicFactor": material_node.metallic,
                    "roughnessFactor": material_node.roughness
                },
                "alphaMode": "OPAQUE",
                "doubleSided": true
            });

            // Apply depth bias for non-substrate materials
            if let Some(f) = factor {
                material_json["extras"] = json!({
                    "polygonOffset": true,
                    "polygonOffsetFactor": f,
                    "polygonOffsetUnits": f,
                    "renderOrder": (10 - precedence) as i32
                });
            }

            materials_array.push(material_json);
        }
    }

    let mut all_vertices: Vec<f32> = Vec::new();
    let mut all_normals: Vec<f32> = Vec::new();
    let mut all_tangents: Vec<f32> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut accessors_array = Vec::new();
    let mut meshes_array = Vec::new();
    let mut nodes_array = Vec::new();

    // 2. Single Pass Geometry Processing (using optimized meshes)
    for (idx, mesh) in optimized_meshes.iter().enumerate() { // FIXED: Iterate over optimized_meshes
        let mat_idx = mat_map.get(&mesh.material_name).copied().unwrap_or(0);

        let v_start = all_vertices.len() / 3; 
        let i_start = all_indices.len(); 

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        // v0.1.7: Flatten mesh to triangles to support per-face normals/tangents
        let mut local_vertex_count = 0;
        for face in &mesh.faces {
            // Get triangle indices for this face
            let tris = if face.vertices.len() == 4 {
                vec![
                    (face.vertices[0], face.vertices[1], face.vertices[2]),
                    (face.vertices[0], face.vertices[2], face.vertices[3]),
                ]
            } else {
                vec![(face.vertices[0], face.vertices[1], face.vertices[2])]
            };

            for (i1, i2, i3) in tris {
                let v1 = &mesh.vertices[i1];
                let v2 = &mesh.vertices[i2];
                let v3 = &mesh.vertices[i3];

                // Calculate Normal
                let ax = (v2.x - v1.x) as f32;
                let ay = (v2.y - v1.y) as f32;
                let az = (v2.z - v1.z) as f32;
                let bx = (v3.x - v1.x) as f32;
                let by = (v3.y - v1.y) as f32;
                let bz = (v3.z - v1.z) as f32;

                let mut nx = ay * bz - az * by;
                let mut ny = az * bx - ax * bz;
                let mut nz = ax * by - ay * bx;
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                if len > 0.0 {
                    nx /= len;
                    ny /= len;
                    nz /= len;
                }

                // Calculate Tangent (Simple orthogonal vector)
                let (mut tx, mut ty, mut tz) = if ny.abs() < 0.9 {
                    // T = normalize(cross(Y, N))
                    (nz, 0.0, -nx)
                } else {
                    // T = normalize(cross(Z, N))
                    (-ny, nx, 0.0)
                };
                let tlen = (tx * tx + ty * ty + tz * tz).sqrt();
                if tlen > 0.0 {
                    tx /= tlen;
                    ty /= tlen;
                    tz /= tlen;
                }

                // Add 3 vertices for the triangle
                for v in [v1, v2, v3] {
                    let vx = v.x as f32 / 1000.0;
                    let vy = v.y as f32 / 1000.0;
                    let vz = v.z as f32 / 1000.0;
                    all_vertices.extend_from_slice(&[vx, vy, vz]);
                    all_normals.extend_from_slice(&[nx, ny, nz]);
                    all_tangents.extend_from_slice(&[tx, ty, tz, 1.0]); // VEC4
                    
                    min_x = min_x.min(vx);
                    max_x = max_x.max(vx);
                    min_y = min_y.min(vy);
                    max_y = max_y.max(vy);
                    min_z = min_z.min(vz);
                    max_z = max_z.max(vz);
                    
                    all_indices.push(local_vertex_count as u32);
                    local_vertex_count += 1;
                }
            }
        }

        let vertex_count = local_vertex_count;
        let index_count = vertex_count; // Since we flattened, index count = vertex count

        // Vertex Position Accessor
        accessors_array.push(json!({
            "bufferView": 0,
            "byteOffset": v_start * 3 * 4,
            "componentType": 5126, // FLOAT
            "count": vertex_count,
            "type": "VEC3",
            "min": [min_x, min_y, min_z],
            "max": [max_x, max_y, max_z]
        }));

        // Vertex Normal Accessor
        accessors_array.push(json!({
            "bufferView": 1,
            "byteOffset": v_start * 3 * 4,
            "componentType": 5126, // FLOAT
            "count": vertex_count,
            "type": "VEC3"
        }));

        // Vertex Tangent Accessor
        accessors_array.push(json!({
            "bufferView": 2,
            "byteOffset": v_start * 4 * 4,
            "componentType": 5126, // FLOAT
            "count": vertex_count,
            "type": "VEC4"
        }));

        // Index accessor
        accessors_array.push(json!({
            "bufferView": 3,
            "byteOffset": i_start * 4,
            "componentType": 5125, // UNSIGNED_INT
            "count": index_count,
            "type": "SCALAR"
        }));

        meshes_array.push(json!({
            "name": mesh.name,
            "primitives": [{
                "attributes": {
                    "POSITION": idx * 4,
                    "NORMAL": idx * 4 + 1,
                    "TANGENT": idx * 4 + 2
                },
                "indices": idx * 4 + 3,
                "material": mat_idx
            }]
        }));

        nodes_array.push(json!({
            "mesh": idx
        }));
    }

    // 3. Assemble glTF JSON with Multi-BufferView Architecture
    let v_size = all_vertices.len() * 4;
    let n_size = all_normals.len() * 4;
    let t_size = all_tangents.len() * 4;
    let i_size = all_indices.len() * 4;

    let gltf = json!({
        "asset": {
            "version": "2.0",
            "generator": "HWS v0.1.7"
        },
        "extensionsUsed": [
            "KHR_materials_ior",
            "KHR_materials_clearcoat",
            "KHR_materials_transmission",
            "KHR_materials_volume",
            "KHR_materials_anisotropy"
        ],
        "scene": 0,
        "scenes": [{
            "nodes": (0..nodes_array.len()).collect::<Vec<_>>()
        }],
        "nodes": nodes_array,
        "meshes": meshes_array,
        "materials": materials_array,
        "buffers": [{
            "byteLength": v_size + n_size + t_size + i_size
        }],
        "bufferViews": [
            {
                "buffer": 0,
                "byteOffset": 0,
                "byteLength": v_size,
                "byteStride": 12,
                "target": 34962
            },
            {
                "buffer": 0,
                "byteOffset": v_size,
                "byteLength": n_size,
                "byteStride": 12,
                "target": 34962
            },
            {
                "buffer": 0,
                "byteOffset": v_size + n_size,
                "byteLength": t_size,
                "byteStride": 16,
                "target": 34962
            },
            {
                "buffer": 0,
                "byteOffset": v_size + n_size + t_size,
                "byteLength": i_size,
                "target": 34963
            }
        ],
        "accessors": accessors_array
    });

    let gltf_json = serde_json::to_string(&gltf).expect("Failed to serialize glTF JSON");

    // 4. Final Binary Packing
    let mut bin_data = Vec::with_capacity(v_size + n_size + t_size + i_size);
    for &v in &all_vertices {
        bin_data.extend_from_slice(&v.to_le_bytes());
    }
    for &n in &all_normals {
        bin_data.extend_from_slice(&n.to_le_bytes());
    }
    for &t in &all_tangents {
        bin_data.extend_from_slice(&t.to_le_bytes());
    }
    for &i in &all_indices {
        bin_data.extend_from_slice(&i.to_le_bytes());
    }

    // 5. Assemble GLB binary container
    let mut glb_data = Vec::new();

    // GLB header
    glb_data.extend_from_slice(b"glTF");
    glb_data.extend_from_slice(&2u32.to_le_bytes());

    // JSON chunk
    let json_bytes = gltf_json.as_bytes();
    let json_padding = (4 - (json_bytes.len() % 4)) % 4;
    let json_length = json_bytes.len() + json_padding;

    // Binary chunk padding
    let bin_padding = (4 - (bin_data.len() % 4)) % 4;
    let bin_length = bin_data.len() + bin_padding;

    // Total length
    let total_length = 12 + 8 + json_length + 8 + bin_length;
    glb_data.extend_from_slice(&(total_length as u32).to_le_bytes());

    // JSON chunk header
    glb_data.extend_from_slice(&(json_length as u32).to_le_bytes());
    glb_data.extend_from_slice(b"JSON");
    glb_data.extend_from_slice(json_bytes);
    glb_data.extend(std::iter::repeat_n(b' ', json_padding));

    // Binary chunk header
    glb_data.extend_from_slice(&(bin_length as u32).to_le_bytes());
    glb_data.extend_from_slice(b"BIN\0");
    glb_data.extend_from_slice(&bin_data);
    glb_data.extend(std::iter::repeat_n(0u8, bin_padding));

    glb_data
}
