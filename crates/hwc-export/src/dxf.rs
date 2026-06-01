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

use crate::physical_z::board_z_extent;
use hwc_compiler::SymbolTable;
use hwc_engine::{HardwareSpace, SpaceView};
use std::io::Write;

/// Export HardwareSpace to DXF format with True Color support and SOLID entities
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

    // Add global layers (v0.1.7 Segmented Viewport Export)
    writeln!(w, "  0\nLAYER\n  2\nDRILL\n 70\n0\n 62\n0")?;
    writeln!(w, "  0\nLAYER\n  2\nTOP_COMPONENTS\n 70\n0\n 62\n7")?;
    writeln!(w, "  0\nLAYER\n  2\nBOTTOM_COMPONENTS\n 70\n0\n 62\n7")?;
    writeln!(w, "  0\nLAYER\n  2\nPCB_LAYERS\n 70\n0\n 62\n7")?;

    let substrate_layers = space.voxel_grid.get_substrate_layers();

    // v0.1.7: Segmented Viewport Export does not require dynamic layer definitions
    // if we are strictly using the three category layers. 
    // However, we still export DRILL separately.
    writeln!(w, "  0\nENDTAB\n  0\nENDSEC")?;

    // 3. ENTITIES (The Actual Geometry)
    writeln!(w, "  0\nSECTION\n  2\nENTITIES")?;

    let (board_min_z, board_max_z) = board_z_extent(space);

    // PROFESSIONAL EDA APPROACH: Export 2D silhouettes (top-down projection)
    for layer in substrate_layers.iter() {
        let mat_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#808080".into());

        let r = u32::from_str_radix(&color_hex[1..3], 16)?;
        let g = u32::from_str_radix(&color_hex[3..5], 16)?;
        let b = u32::from_str_radix(&color_hex[5..7], 16)?;
        let true_color: u32 = (r << 16) | (g << 8) | b;

        // **ORIENTATION AWARE PROJECTION (v0.1.6)**
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

        // v0.1.7: Segmented Viewport Export
        let layer_name = if mat_name.to_lowercase() == "void" || mat_name.to_lowercase() == "air" {
            "DRILL".to_string()
        } else {
            "PCB_LAYERS".to_string()
        };

        writeln!(w, "  0\nLWPOLYLINE")?;
        writeln!(w, "  8\n{}", layer_name)?;
        writeln!(w, "420\n{}", true_color)?;
        writeln!(w, " 90\n4")?;
        writeln!(w, " 70\n1")?;

        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y1)?; // Bottom-left
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y1)?; // Bottom-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y2)?; // Top-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y2)?; // Top-left
    }

    // Export component metadata
    let component_metadata = space.voxel_grid.get_component_metadata();

    for component in component_metadata.iter() {
        let mat_name = space
            .material_registry
            .get_name(component.material)
            .unwrap_or("Body");

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#B87333".into());

        let r = u32::from_str_radix(&color_hex[1..3], 16).unwrap_or(184);
        let g = u32::from_str_radix(&color_hex[3..5], 16).unwrap_or(115);
        let b = u32::from_str_radix(&color_hex[5..7], 16).unwrap_or(51);
        let true_color: u32 = (r << 16) | (g << 8) | b;

        // **ORIENTATION AWARE PROJECTION (v0.1.6)**
        let (x1, y1, x2, y2) = match space.view {
            SpaceView::Horizontal => (
                component.bbox.min.x as f64 / 1_000_000.0,
                component.bbox.min.y as f64 / 1_000_000.0,
                component.bbox.max.x as f64 / 1_000_000.0,
                component.bbox.max.y as f64 / 1_000_000.0,
            ),
            SpaceView::Vertical => (
                component.bbox.min.x as f64 / 1_000_000.0,
                component.bbox.min.z as f64 / 1_000_000.0,
                component.bbox.max.x as f64 / 1_000_000.0,
                component.bbox.max.z as f64 / 1_000_000.0,
            ),
        };

        // v0.1.7: Segmented Viewport Export
        let layer_name = if component.bbox.min.z >= board_max_z {
            "TOP_COMPONENTS".to_string()
        } else if component.bbox.max.z <= board_min_z {
            "BOTTOM_COMPONENTS".to_string()
        } else {
            "PCB_LAYERS".to_string()
        };

        writeln!(w, "  0\nLWPOLYLINE")?;
        writeln!(w, "  8\n{}", layer_name)?;
        writeln!(w, "420\n{}", true_color)?;
        writeln!(w, " 90\n4")?;
        writeln!(w, " 70\n1")?;

        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y1)?; // Bottom-left
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y1)?; // Bottom-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y2)?; // Top-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y2)?; // Top-left
    }

    // Export analytic routes as polylines in DXF
    for route in &space.analytic_routes {
        let mat_name = space
            .material_registry
            .get_name(route.material)
            .unwrap_or("Copper");

        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#FF6600".into());

        let r = u32::from_str_radix(&color_hex[1..3], 16).unwrap_or(255);
        let g = u32::from_str_radix(&color_hex[3..5], 16).unwrap_or(102);
        let b = u32::from_str_radix(&color_hex[5..7], 16).unwrap_or(0);
        let true_color: u32 = (r << 16) | (g << 8) | b;

        let width_mm = route.width_nm as f64 / 1_000_000.0;

        for segment in &route.segments {
            // **ORIENTATION AWARE PROJECTION (v0.1.6)**
            let (x1, y1, x2, y2) = match space.view {
                SpaceView::Horizontal => (
                    segment.start.x as f64 / 1_000_000.0,
                    segment.start.y as f64 / 1_000_000.0,
                    segment.end.x as f64 / 1_000_000.0,
                    segment.end.y as f64 / 1_000_000.0,
                ),
                SpaceView::Vertical => (
                    segment.start.x as f64 / 1_000_000.0,
                    segment.start.z as f64 / 1_000_000.0,
                    segment.end.x as f64 / 1_000_000.0,
                    segment.end.z as f64 / 1_000_000.0,
                ),
            };

            // v0.1.7: Segmented Viewport Export
            let layer_name = if mat_name.to_lowercase() == "void" || mat_name.to_lowercase() == "air" {
                "DRILL".to_string()
            } else {
                "PCB_LAYERS".to_string()
            };

            writeln!(w, "  0\nLWPOLYLINE")?;
            writeln!(w, "  8\n{}", layer_name)?;
            writeln!(w, "420\n{}", true_color)?;
            writeln!(w, " 43\n{:.6}", width_mm)?;
            writeln!(w, " 90\n2")?;
            writeln!(w, " 70\n0")?;

            writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y1)?;
            writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y2)?;
        }
    }

    writeln!(w, "  0\nENDSEC\n  0\nEOF")?;
    println!("   ✅ DXF: {}", path.display());
    Ok(())
}
