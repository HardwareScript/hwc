use crate::scene_graph::types::{Face, MeshNode, Vertex};
use hwc_engine::geometry_router::entity_graph::CapType;
use hwc_engine::SpaceView;

/// Create a unified plated-through-hole mesh (Limitation 7 Fix)
/// This creates a single mesh containing the inner tube and the top/bottom pads.
#[allow(clippy::too_many_arguments)]
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

    // Add vertices for bottom (z=0)
    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        vertices.push(map_vertex(
            cx + r_bottom_inner * cos_a,
            cy + r_bottom_inner * sin_a,
            cz,
        )); // 0: Inner Bottom
        vertices.push(map_vertex(
            cx + r_bottom_plating * cos_a,
            cy + r_bottom_plating * sin_a,
            cz,
        )); // 1: Plating Bottom
        vertices.push(map_vertex(cx + r_pad * cos_a, cy + r_pad * sin_a, cz)); // 2: Pad Bottom
    }

    // Add vertices for top (z=height)
    let top_offset = vertices.len();
    for i in 0..actual_segments as usize {
        let angle = (i as f64 / actual_segments as f64) * 2.0 * std::f64::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        vertices.push(map_vertex(
            cx + r_top_inner * cos_a,
            cy + r_top_inner * sin_a,
            cz + actual_height,
        )); // 3: Inner Top
        vertices.push(map_vertex(
            cx + r_top_plating * cos_a,
            cy + r_top_plating * sin_a,
            cz + actual_height,
        )); // 4: Plating Top
        vertices.push(map_vertex(
            cx + r_pad * cos_a,
            cy + r_pad * sin_a,
            cz + actual_height,
        )); // 5: Pad Top
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
        let bi_plat = i * 3 + 1;
        let bi_pad = i * 3 + 2;

        let bnext_inner = next * 3;
        let bnext_plat = next * 3 + 1;
        let bnext_pad = next * 3 + 2;

        // Top indices (top_offset + i*3)
        let ti_inner = top_offset + bi_inner;
        let ti_plat = top_offset + bi_plat;
        let ti_pad = top_offset + bi_pad;

        let tnext_inner = top_offset + bnext_inner;
        let tnext_plat = top_offset + bnext_plat;
        let tnext_pad = top_offset + bnext_pad;

        // 1. Inner Wall (facing INWARDS)
        // Correct CCW winding for inner tube: bi_inner -> ti_inner -> tnext_inner -> bnext_inner
        faces.push(Face {
            vertices: vec![bi_inner, ti_inner, tnext_inner, bnext_inner],
        });

        // 2. Plating Wall (facing OUTWARDS)
        // Correct CCW winding for outer tube: bi_plat -> bnext_plat -> tnext_plat -> ti_plat
        faces.push(Face {
            vertices: vec![bi_plat, bnext_plat, tnext_plat, ti_plat],
        });

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
