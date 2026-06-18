//! DXF Export - The Universal CAD Format
//!
//! DXF (Drawing Exchange Format) is the "Old Faithful" of the CAD world:
//! - Single text file format
//! - Supports exact 24-bit True Color (Group Code 420)
//! - Supports native transparency (Group Code 440)
//! - Uses SOLID entities for true filled rectangles (not just outlines)
//! - AutoCAD Color Index (ACI) backup for legacy viewers
//! - Nearly impossible to get wrong
//! - Opens in Autodesk Viewer, KiCad, AutoCAD, and virtually every CAD tool
//!
//! This is the most reliable format for visual verification of Hardware Script output.
//! The SOLID entity ensures proper rendering, and transparency is baked into the layer
//! definitions for tools that support it (AC1018+).

use crate::geometry_union::{circle_to_path, rect_to_path};
use clipper2_rust::FillRule;
use hwc_compiler::SymbolTable;
use hwc_engine::voxel_grid::{CapType, SubstrateLayerShape, SubstrateLayerType};
use hwc_engine::{HardwareSpace, SpaceView};
use rustc_hash::FxHashMap;
use std::io::Write;

/// Export HardwareSpace to DXF format with True Color support and unioned copper contours
pub fn export(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &std::path::Path,
    _space_def: Option<&hwc_parser::SpaceDefinition>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = output_dir.join("board.dxf");
    let file = std::fs::File::create(&path)?;
    let mut w = std::io::BufWriter::new(file);

    // 1. Header
    writeln!(
        w,
        "  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1018\n  0\nENDSEC"
    )?;

    // 2. TABLES (Layer Definitions with Color and Transparency)
    writeln!(w, "  0\nSECTION\n  2\nTABLES\n  0\nTABLE\n  2\nLAYER")?;

    // Add global layers
    writeln!(w, "  0\nLAYER\n  2\nDRILL\n 70\n0\n 62\n0")?;
    writeln!(w, "  0\nLAYER\n  2\nTOP_COMPONENTS\n 70\n0\n 62\n7")?;
    writeln!(w, "  0\nLAYER\n  2\nBOTTOM_COMPONENTS\n 70\n0\n 62\n7")?;
    writeln!(w, "  0\nLAYER\n  2\nPCB_LAYERS\n 70\n0\n 62\n7")?;

    let substrate_layers = space.voxel_grid.get_substrate_layers();

    writeln!(w, "  0\nENDTAB\n  0\nENDSEC")?;

    // 3. ENTITIES (The Actual Geometry)
    writeln!(w, "  0\nSECTION\n  2\nENTITIES")?;

    // --- STRATEGY A: Union copper pours for clean DXF contours ---
    // Collect copper pools keyed by (z_min, z_max, material, net)
    let mut copper_pools: FxHashMap<
        (i64, i64, hwc_engine::voxel_grid::MaterialId, u32),
        Vec<clipper2_rust::Path64>,
    > = FxHashMap::default();
    let mut via_holes: FxHashMap<
        (i64, i64, hwc_engine::voxel_grid::MaterialId, u32),
        Vec<clipper2_rust::Path64>,
    > = FxHashMap::default();

    // Gather copper pours
    for layer in substrate_layers {
        if layer.layer_type == SubstrateLayerType::Pour && layer.net != 0 {
            let key = (
                layer.bbox.min.z,
                layer.bbox.max.z,
                layer.material,
                layer.net,
            );

            // v0.1.8: If layer has child regions, emit one path per region.
            let region_bboxes: Vec<_> = if layer.regions.is_empty() {
                vec![layer.bbox]
            } else {
                layer.regions.to_vec()
            };

            for region_bbox in &region_bboxes {
                let mut path = match layer.shape {
                    SubstrateLayerShape::Rect => rect_to_path(region_bbox),
                    SubstrateLayerShape::Circle { radius } => {
                        let cx = (region_bbox.min.x + region_bbox.max.x) / 2;
                        let cy = (region_bbox.min.y + region_bbox.max.y) / 2;
                        circle_to_path(cx, cy, radius, 64)
                    }
                    _ => continue,
                };

                // v0.1.9: Subtract cutouts from the path before adding to the pool
                if !layer.cutouts.is_empty() {
                    let mut hole_paths = Vec::new();
                    for cutout in &layer.cutouts {
                        match cutout.shape {
                            SubstrateLayerShape::Rect => {
                                hole_paths.push(rect_to_path(&cutout.bbox))
                            }
                            SubstrateLayerShape::Polygon {
                                ref outer_contour, ..
                            } => {
                                hole_paths.push(outer_contour.clone());
                            }
                            _ => {}
                        }
                    }
                    if !hole_paths.is_empty() {
                        let diff = clipper2_rust::difference_64(
                            &vec![path.clone()],
                            &hole_paths,
                            FillRule::NonZero,
                        );
                        if !diff.is_empty() {
                            path = diff[0].clone();
                        }
                    }
                }

                copper_pools.entry(key).or_default().push(path);
            }
        }
    }

    // Gather via caps
    for layer in substrate_layers {
        if layer.layer_type == SubstrateLayerType::Contact && layer.net != 0 {
            if let SubstrateLayerShape::Tube {
                inner_diameter,
                pad_diameter,
                top_cap,
                bottom_cap,
                ..
            } = layer.shape
            {
                let cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let pad_radius = pad_diameter as i64 / 2;
                let inner_radius = inner_diameter as i64 / 2;
                let copper_thickness = 35_000;

                if top_cap != CapType::None {
                    let target_key = (
                        layer.bbox.max.z - copper_thickness,
                        layer.bbox.max.z,
                        layer.material,
                        layer.net,
                    );
                    copper_pools
                        .entry(target_key)
                        .or_default()
                        .push(circle_to_path(cx, cy, pad_radius, 64));
                    if top_cap == CapType::Annular {
                        via_holes
                            .entry(target_key)
                            .or_default()
                            .push(circle_to_path(cx, cy, inner_radius, 64));
                    }
                }

                if bottom_cap != CapType::None {
                    let target_key = (
                        layer.bbox.min.z,
                        layer.bbox.min.z + copper_thickness,
                        layer.material,
                        layer.net,
                    );
                    copper_pools
                        .entry(target_key)
                        .or_default()
                        .push(circle_to_path(cx, cy, pad_radius, 64));
                    if bottom_cap == CapType::Annular {
                        via_holes
                            .entry(target_key)
                            .or_default()
                            .push(circle_to_path(cx, cy, inner_radius, 64));
                    }
                }
            }
        }
    }

    // Export unioned copper contours
    for ((z_min_nm, z_max_nm, material_id, net_raw), paths) in &copper_pools {
        let mat_name = space
            .material_registry
            .get_name(*material_id)
            .unwrap_or("Copper");
        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#B87333".into());
        let true_color = parse_true_color(&color_hex);

        let mut clipper = clipper2_rust::Clipper64::new();
        clipper.add_subject(paths);
        if let Some(holes) = via_holes.get(&(*z_min_nm, *z_max_nm, *material_id, *net_raw)) {
            clipper.add_clip(holes);
        }

        let mut final_paths = clipper2_rust::Paths64::new();
        clipper.execute(
            clipper2_rust::ClipType::Difference,
            clipper2_rust::FillRule::NonZero,
            &mut final_paths,
            None,
        );
        if final_paths.is_empty() {
            continue;
        }

        for path in &final_paths {
            let point_count = path.len();
            if point_count < 3 {
                continue;
            }

            writeln!(w, "  0\nLWPOLYLINE")?;
            writeln!(w, "  8\nPCB_LAYERS")?;
            writeln!(w, "420\n{}", true_color)?;
            writeln!(w, " 90\n{}", point_count)?;
            writeln!(w, " 70\n1")?; // Closed polyline

            for pt in path {
                let (x, y) = match space.view {
                    SpaceView::Horizontal => (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0),
                    SpaceView::Vertical => {
                        (pt.x as f64 / 1_000_000.0, *z_min_nm as f64 / 1_000_000.0)
                    }
                };
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x, y)?;
            }
        }
    }

    // Export non-copper substrate layers (FR4, solder mask, etc.) and via drill holes
    for layer in substrate_layers {
        // Skip copper pours and contacts — already handled by the union pool above
        if layer.layer_type == SubstrateLayerType::Pour && layer.net != 0 {
            continue;
        }
        if layer.layer_type == SubstrateLayerType::Contact {
            continue;
        }

        let mat_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");
        if mat_name.to_lowercase() == "void" || mat_name.to_lowercase() == "air" {
            continue;
        }

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#808080".into());
        let true_color = parse_true_color(&color_hex);

        let layer_name = "PCB_LAYERS";

        match &layer.shape {
            SubstrateLayerShape::Rect => {
                // Export as closed LWPOLYLINE rectangle
                let (x1, y1, x2, y2) = match space.view {
                    SpaceView::Horizontal => (
                        layer.bbox.min.x as f64 / 1_000_000.0,
                        layer.bbox.min.y as f64 / 1_000_000.0,
                        layer.bbox.max.x as f64 / 1_000_000.0,
                        layer.bbox.max.y as f64 / 1_000_000.0,
                    ),
                    SpaceView::Vertical => (
                        layer.bbox.min.x as f64 / 1_000_000.0,
                        layer.bbox.min.z as f64 / 1_000_000.0,
                        layer.bbox.max.x as f64 / 1_000_000.0,
                        layer.bbox.max.z as f64 / 1_000_000.0,
                    ),
                };

                writeln!(w, "  0\nLWPOLYLINE")?;
                writeln!(w, "  8\n{}", layer_name)?;
                writeln!(w, "420\n{}", true_color)?;
                writeln!(w, " 90\n4")?;
                writeln!(w, " 70\n1")?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y1)?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y1)?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y2)?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y2)?;
            }
            SubstrateLayerShape::Circle { radius } => {
                // Export as CIRCLE entity
                let cx = (layer.bbox.min.x + layer.bbox.max.x) as f64 / 2_000_000.0;
                let cy = match space.view {
                    SpaceView::Horizontal => {
                        (layer.bbox.min.y + layer.bbox.max.y) as f64 / 2_000_000.0
                    }
                    SpaceView::Vertical => {
                        (layer.bbox.min.z + layer.bbox.max.z) as f64 / 2_000_000.0
                    }
                };
                let radius_mm = *radius as f64 / 1_000_000.0;

                writeln!(w, "  0\nCIRCLE")?;
                writeln!(w, "  8\n{}", layer_name)?;
                writeln!(w, "420\n{}", true_color)?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx, cy)?;
                writeln!(w, " 40\n{:.6}", radius_mm)?;
            }
            SubstrateLayerShape::Polygon {
                ref outer_contour,
                ref holes,
                ..
            } => {
                let center_x_nm = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let center_y_nm = (layer.bbox.min.y + layer.bbox.max.y) / 2;
                let cx_mm = center_x_nm as f64 / 1_000_000.0;
                let cy_mm = center_y_nm as f64 / 1_000_000.0;

                let outer_points: Vec<(f64, f64)> = outer_contour
                    .iter()
                    .map(|p| {
                        (
                            p.x as f64 / 1_000_000.0 + cx_mm,
                            p.y as f64 / 1_000_000.0 + cy_mm,
                        )
                    })
                    .collect();

                if outer_points.len() >= 3 {
                    writeln!(w, "  0\nLWPOLYLINE")?;
                    writeln!(w, "  8\n{}", layer_name)?;
                    writeln!(w, "420\n{}", true_color)?;
                    writeln!(w, " 90\n{}", outer_points.len())?;
                    writeln!(w, " 70\n1")?;
                    for (x, y) in &outer_points {
                        let (x_out, y_out) = match space.view {
                            SpaceView::Horizontal => (*x, *y),
                            SpaceView::Vertical => (*x, layer.bbox.min.z as f64 / 1_000_000.0),
                        };
                        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x_out, y_out)?;
                    }
                }

                for hole in holes.iter() {
                    let hole_points: Vec<(f64, f64)> = hole
                        .iter()
                        .map(|p| {
                            (
                                p.x as f64 / 1_000_000.0 + cx_mm,
                                p.y as f64 / 1_000_000.0 + cy_mm,
                            )
                        })
                        .collect();
                    if hole_points.len() >= 3 {
                        writeln!(w, "  0\nLWPOLYLINE")?;
                        writeln!(w, "  8\nDRILL")?;
                        writeln!(w, "420\n0")?;
                        writeln!(w, " 90\n{}", hole_points.len())?;
                        writeln!(w, " 70\n1")?;
                        for (x, y) in &hole_points {
                            let (x_out, y_out) = match space.view {
                                SpaceView::Horizontal => (*x, *y),
                                SpaceView::Vertical => (*x, layer.bbox.min.z as f64 / 1_000_000.0),
                            };
                            writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x_out, y_out)?;
                        }
                    }
                }
            }
            SubstrateLayerShape::Tube {
                outer_diameter,
                inner_diameter,
                ..
            } => {
                // Export outer circle and inner hole circle
                let cx = (layer.bbox.min.x + layer.bbox.max.x) as f64 / 2_000_000.0;
                let cy = match space.view {
                    SpaceView::Horizontal => {
                        (layer.bbox.min.y + layer.bbox.max.y) as f64 / 2_000_000.0
                    }
                    SpaceView::Vertical => {
                        (layer.bbox.min.z + layer.bbox.max.z) as f64 / 2_000_000.0
                    }
                };
                let outer_r = *outer_diameter as f64 / 2_000_000.0;
                let inner_r = *inner_diameter as f64 / 2_000_000.0;

                // Outer circle
                writeln!(w, "  0\nCIRCLE")?;
                writeln!(w, "  8\n{}", layer_name)?;
                writeln!(w, "420\n{}", true_color)?;
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx, cy)?;
                writeln!(w, " 40\n{:.6}", outer_r)?;

                // Inner hole circle (drill)
                if inner_r > 0.0 {
                    writeln!(w, "  0\nCIRCLE")?;
                    writeln!(w, "  8\nDRILL")?;
                    writeln!(w, "420\n0")?;
                    writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx, cy)?;
                    writeln!(w, " 40\n{:.6}", inner_r)?;
                }
            }
        }
    }

    writeln!(w, "  0\nENDSEC\n  0\nEOF")?;
    println!("   ✅ DXF: {}", path.display());
    Ok(())
}

/// Parse a hex color string like "#RRGGBB" into a DXF 24-bit true color integer.
fn parse_true_color(hex: &str) -> u32 {
    let r = u32::from_str_radix(&hex[1..3], 16).unwrap_or(128);
    let g = u32::from_str_radix(&hex[3..5], 16).unwrap_or(128);
    let b = u32::from_str_radix(&hex[5..7], 16).unwrap_or(128);
    (r << 16) | (g << 8) | b
}
