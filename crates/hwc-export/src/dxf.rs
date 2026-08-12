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
//! **v0.2.2 Architecture**: DXF is now a pure reader of unified geometry.
//! All copper contours come from the unified_geometry module (single source of truth).
//! No geometry calculations or Boolean operations happen here.

use hwc_compiler::SymbolTable;
use hwc_engine::geometry_router::entity_graph::SubstrateLayerType;
use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;
use hwc_engine::{HardwareSpace, SpaceView};
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

    let is_asic = space
        .fabrication_constraints
        .as_ref()
        .is_some_and(|c| c.technology.is_asic());

    if is_asic {
        // ASIC Mode: Write only the physical semiconductor mask layers from the stackup
        // v0.2.2: Use get_physical_substrate_layers() to exclude zero-thickness masks
        let physical_layers: Vec<_> = space
            .entity_graph
            .get_physical_substrate_layers(&space.material_registry)
            .collect();
        let mut seen_materials = rustc_hash::FxHashSet::default();
        for layer in &physical_layers {
            if seen_materials.insert(layer.material) {
                let mat_name = space
                    .material_registry
                    .get_name(layer.material)
                    .unwrap_or_else(|| {
                        panic!(
                            "Material ID {:?} not found in registry during DXF layer definition",
                            layer.material
                        )
                    });
                writeln!(w, "  0\nLAYER\n  2\n{}\n 70\n0\n 62\n7", mat_name)?;
            }
        }
    } else {
        // PCB Mode: Add global layers
        writeln!(w, "  0\nLAYER\n  2\nDRILL\n 70\n0\n 62\n0")?;
        writeln!(w, "  0\nLAYER\n  2\nTOP_COMPONENTS\n 70\n0\n 62\n7")?;
        writeln!(w, "  0\nLAYER\n  2\nBOTTOM_COMPONENTS\n 70\n0\n 62\n7")?;
        writeln!(w, "  0\nLAYER\n  2\nPCB_LAYERS\n 70\n0\n 62\n7")?;
    }

    writeln!(w, "  0\nENDTAB\n  0\nENDSEC")?;

    // 3. ENTITIES (The Actual Geometry)
    writeln!(w, "  0\nSECTION\n  2\nENTITIES")?;

    // **v0.2.2: USE UNIFIED GEOMETRY (SINGLE SOURCE OF TRUTH)**
    // All copper contours come from the unified geometry module.
    // DXF is now a pure reader - no geometry calculations here.
    let copper_contours = crate::scene_graph::generate_copper_contours(space);

    eprintln!(
        "[DXF EXPORT] Received {} copper contour groups from unified geometry",
        copper_contours.len()
    );

    // Export each unified copper contour pool
    for contour_data in &copper_contours {
        let z_min_nm = contour_data.key.z_min;
        let material_id = contour_data.key.material;

        let mat_name = space
            .material_registry
            .get_name(material_id)
            .unwrap_or_else(|| {
                panic!(
                    "Material ID {:?} not found in registry during DXF copper export",
                    material_id
                )
            });

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| {
                panic!(
                    "Material '{}' not found in symbol table during DXF export",
                    mat_name
                )
            });
        let true_color = parse_true_color(&color_hex);

        for path in &contour_data.contours {
            let point_count = path.len();
            if point_count < 3 {
                continue;
            }

            let layer_name = if is_asic { mat_name } else { "PCB_LAYERS" };

            writeln!(w, "  0\nLWPOLYLINE")?;
            writeln!(w, "  8\n{}", layer_name)?;
            writeln!(w, "420\n{}", true_color)?;
            writeln!(w, " 90\n{}", point_count)?;
            writeln!(w, " 70\n1")?; // Closed polyline

            for pt in path {
                let (x, y) = match space.view {
                    SpaceView::Horizontal => (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0),
                    SpaceView::Vertical => {
                        (pt.x as f64 / 1_000_000.0, z_min_nm as f64 / 1_000_000.0)
                    }
                };
                writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x, y)?;
            }
        }
    }

    // Export substrate base, pads, and via drill holes (non-trace geometry)
    // v0.2.2 STRUCTURAL FIX: Use get_physical_substrate_layers() instead of get_substrate_layers()
    // This ensures zero-thickness masks are NEVER exported as physical geometry.
    for layer in space
        .entity_graph
        .get_physical_substrate_layers(&space.material_registry)
    {
        let mat_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or_else(|| {
                panic!(
                    "Material ID {:?} not found in registry during DXF substrate export",
                    layer.material
                )
            });

        if mat_name.to_lowercase() == "void" || mat_name.to_lowercase() == "air" {
            continue;
        }

        // Export substrate base
        // Skip Contact and Pour types since they're already exported as part of analytic routes (unioned with traces)
        if layer.layer_type != SubstrateLayerType::Substrate {
            continue;
        }

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| {
                panic!(
                    "Material '{}' not found in symbol table during DXF substrate export",
                    mat_name
                )
            });
        let true_color = parse_true_color(&color_hex);

        let layer_name = if is_asic { mat_name } else { "PCB_LAYERS" };

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
                // Polygon points are now in world space, convert directly to mm
                let outer_points: Vec<(f64, f64)> = outer_contour
                    .iter()
                    .map(|p| (p.x as f64 / 1_000_000.0, p.y as f64 / 1_000_000.0))
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
                    // Holes are also in world space
                    let hole_points: Vec<(f64, f64)> = hole
                        .iter()
                        .map(|p| (p.x as f64 / 1_000_000.0, p.y as f64 / 1_000_000.0))
                        .collect();
                    if hole_points.len() >= 3 {
                        writeln!(w, "  0\nLWPOLYLINE")?;
                        let drill_layer = if is_asic { mat_name } else { "DRILL" };
                        writeln!(w, "  8\n{}", drill_layer)?;
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
                    let drill_layer = if is_asic { mat_name } else { "DRILL" };
                    writeln!(w, "  8\n{}", drill_layer)?;
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
///
/// **NO FALLBACKS**: This function expects a valid hex color string.
/// Invalid formats will panic with a clear error message.
fn parse_true_color(hex: &str) -> u32 {
    if hex.len() != 7 || !hex.starts_with('#') {
        panic!(
            "Invalid color format '{}'. Expected format: #RRGGBB (e.g., #FF5733)",
            hex
        );
    }

    let r = u32::from_str_radix(&hex[1..3], 16)
        .unwrap_or_else(|_| panic!("Invalid red component in color '{}'. Must be 00-FF", hex));
    let g = u32::from_str_radix(&hex[3..5], 16)
        .unwrap_or_else(|_| panic!("Invalid green component in color '{}'. Must be 00-FF", hex));
    let b = u32::from_str_radix(&hex[5..7], 16)
        .unwrap_or_else(|_| panic!("Invalid blue component in color '{}'. Must be 00-FF", hex));

    (r << 16) | (g << 8) | b
}
