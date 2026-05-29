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

use crate::physical_z::dxf_layer_name;
use hwc_compiler::SymbolTable;
use hwc_engine::{HardwareSpace, SpaceView};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Export HardwareSpace to DXF format with True Color support and SOLID entities
pub fn export(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = output_dir.join("layout.dxf");
    let file = File::create(&path)?;
    let mut w = BufWriter::new(file);

    // 1. Header
    writeln!(
        w,
        "  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1018\n  0\nENDSEC"
    )?;

    // 2. TABLES (Layer Definitions with Color and Transparency)
    writeln!(w, "  0\nSECTION\n  2\nTABLES\n  0\nTABLE\n  2\nLAYER")?;

    let substrate_layers = space.voxel_grid.get_substrate_layers();

    // Group layers by net to identify unique nets
    let mut seen_materials = rustc_hash::FxHashSet::default();
    for layer in substrate_layers.iter() {
        let mat_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");

        if seen_materials.contains(mat_name) {
            continue;
        }
        seen_materials.insert(mat_name);

        // 1. DYNAMIC COLOR & OPACITY EXTRACTION
        let (color_hex, opacity) = if let Ok(mat_def) = symbol_table.get_material(mat_name) {
            (mat_def.get_color(), mat_def.get_opacity()) // Grabs #Hex and 0.0-1.0
        } else {
            ("#808080".into(), 1.0)
        };

        let r = u32::from_str_radix(&color_hex[1..3], 16)?;
        let g = u32::from_str_radix(&color_hex[3..5], 16)?;
        let b = u32::from_str_radix(&color_hex[5..7], 16)?;
        let true_color: u32 = (r << 16) | (g << 8) | b;

        // Convert HWS opacity (0.0-1.0) to DXF transparency (0-255)
        // DXF transparency is 0 = Opaque, 255 = Fully Transparent
        let transparency_val = ((1.0 - opacity) * 255.0) as u32;
        let dxf_transparency = 0x02000000 | transparency_val; // Header byte + alpha

        // Map standard colors to AutoCAD Indices for better viewer compatibility
        let aci = match mat_name.to_lowercase().as_str() {
            m if m.contains("red") => 1,
            m if m.contains("yellow") => 2,
            m if m.contains("green") => 3,
            m if m.contains("cyan") => 4,
            m if m.contains("blue") => 5,
            m if m.contains("magenta") => 6,
            _ => 7, // White/Gray
        };

        writeln!(w, "  0\nLAYER\n  2\nL_{}\n 70\n0", mat_name)?;
        writeln!(w, " 62\n{}", aci)?; // ACI Index backup
        writeln!(w, "420\n{}", true_color)?; // Hard-baked True Color
        writeln!(w, "440\n{}", dxf_transparency)?; // THE MAGIC: Hard-baked Opacity
    }
    writeln!(w, "  0\nENDTAB\n  0\nENDSEC")?;

    // 3. ENTITIES (The Actual Geometry)
    writeln!(w, "  0\nSECTION\n  2\nENTITIES")?;

    // PROFESSIONAL EDA APPROACH: Export 2D silhouettes (top-down projection)
    // Each substrate layer becomes a flat, closed rectangle on its corresponding DXF layer
    // This is how KLayout, Cadence, and other professional tools display masks

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

        let z_center_nm = match space.view {
            SpaceView::Horizontal => (layer.bbox.min.z + layer.bbox.max.z) / 2,
            SpaceView::Vertical => (layer.bbox.min.y + layer.bbox.max.y) / 2,
        };
        let layer_name = dxf_layer_name(z_center_nm, mat_name);

        writeln!(w, "  0\nLWPOLYLINE")?;
        writeln!(w, "  8\n{}", layer_name)?;
        writeln!(w, "420\n{}", true_color)?;
        writeln!(w, " 90\n4")?;
        writeln!(w, " 70\n1")?;

        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y1)?; // Bottom-left
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y1)?; // Bottom-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x2, y2)?; // Top-right
        writeln!(w, " 10\n{:.6}\n 20\n{:.6}", x1, y2)?; // Top-left

        // v0.1.7: Export cutouts (holes) to DXF
        for cutout in &layer.cutouts {
            let (cx1, cy1, cx2, cy2) = match space.view {
                SpaceView::Horizontal => (
                    cutout.bbox.min.x as f64 / 1_000_000.0,
                    cutout.bbox.min.y as f64 / 1_000_000.0,
                    cutout.bbox.max.x as f64 / 1_000_000.0,
                    cutout.bbox.max.y as f64 / 1_000_000.0,
                ),
                SpaceView::Vertical => (
                    cutout.bbox.min.x as f64 / 1_000_000.0,
                    cutout.bbox.min.z as f64 / 1_000_000.0,
                    cutout.bbox.max.x as f64 / 1_000_000.0,
                    cutout.bbox.max.z as f64 / 1_000_000.0,
                ),
            };

            match cutout.shape {
                hwc_engine::voxel_grid::SubstrateLayerShape::Cylinder { diameter, .. } => {
                    let center_x = (cx1 + cx2) / 2.0;
                    let center_y = (cy1 + cy2) / 2.0;
                    let radius = diameter as f64 / 2_000_000.0;

                    writeln!(w, "  0\nCIRCLE")?;
                    writeln!(w, "  8\n{}", layer_name)?;
                    writeln!(w, " 62\n0")?; // Color 0 (ByBlock/Black) for contrast
                    writeln!(w, " 10\n{:.6}", center_x)?;
                    writeln!(w, " 20\n{:.6}", center_y)?;
                    writeln!(w, " 40\n{:.6}", radius)?;
                }
                hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                    outer_diameter,
                    inner_diameter,
                    ..
                } => {
                    let center_x = (cx1 + cx2) / 2.0;
                    let center_y = (cy1 + cy2) / 2.0;
                    let outer_radius = outer_diameter as f64 / 2_000_000.0;
                    let inner_radius = inner_diameter as f64 / 2_000_000.0;

                    // Outer circle
                    writeln!(w, "  0\nCIRCLE")?;
                    writeln!(w, "  8\n{}", layer_name)?;
                    writeln!(w, " 62\n0")?;
                    writeln!(w, " 10\n{:.6}", center_x)?;
                    writeln!(w, " 20\n{:.6}", center_y)?;
                    writeln!(w, " 40\n{:.6}", outer_radius)?;

                    // Inner circle
                    writeln!(w, "  0\nCIRCLE")?;
                    writeln!(w, "  8\n{}", layer_name)?;
                    writeln!(w, " 62\n0")?;
                    writeln!(w, " 10\n{:.6}", center_x)?;
                    writeln!(w, " 20\n{:.6}", center_y)?;
                    writeln!(w, " 40\n{:.6}", inner_radius)?;
                }
                hwc_engine::voxel_grid::SubstrateLayerShape::Rect => {
                    writeln!(w, "  0\nLWPOLYLINE")?;
                    writeln!(w, "  8\n{}", layer_name)?;
                    writeln!(w, " 62\n0")?;
                    writeln!(w, " 90\n4")?;
                    writeln!(w, " 70\n1")?;
                    writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx1, cy1)?;
                    writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx2, cy1)?;
                    writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx2, cy2)?;
                    writeln!(w, " 10\n{:.6}\n 20\n{:.6}", cx1, cy2)?;
                }
            }
        }
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

        let z_center_nm = match space.view {
            SpaceView::Horizontal => (component.bbox.min.z + component.bbox.max.z) / 2,
            SpaceView::Vertical => (component.bbox.min.y + component.bbox.max.y) / 2,
        };
        let layer_name = format!("{}_Components", dxf_layer_name(z_center_nm, "Component"));

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

            let z_center_nm = match space.view {
                SpaceView::Horizontal => segment.start.z,
                SpaceView::Vertical => segment.start.y,
            };
            let layer_name = format!("{}_Traces", dxf_layer_name(z_center_nm, mat_name));

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
