//! KLayout Layer Properties (.lyp) Export
//!
//! The .lyp file is KLayout's native format for defining layer display properties
//! including colors, fill patterns, and visibility. This file must be loaded
//! alongside the DXF to get proper color display in KLayout.
//!
//! Usage: File -> Load Layer Properties -> select the .lyp file

use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Export KLayout Layer Properties file with colors from Hardware Script
pub fn export(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = output_dir.join("layout.lyp");
    let file = File::create(&path)?;
    let mut w = BufWriter::new(file);

    // XML header
    writeln!(w, "<?xml version=\"1.0\" encoding=\"utf-8\"?>")?;
    writeln!(w, "<layer-properties>")?;

    let substrate_layers = space.voxel_grid.get_substrate_layers();
    
    for layer in substrate_layers.iter() {
        let mat_name = space
            .material_registry
            .get_name(layer.material)
            .unwrap_or("Unknown");
        let color_hex = symbol_table
            .get_material(mat_name)
            .map(|m| m.get_color())
            .unwrap_or_else(|_| "#808080".into());

        writeln!(w, " <properties>")?;
        writeln!(w, "  <frame-color>{}</frame-color>", color_hex)?;
        writeln!(w, "  <fill-color>{}</fill-color>", color_hex)?;
        writeln!(w, "  <frame-brightness>0</frame-brightness>")?;
        writeln!(w, "  <fill-brightness>0</fill-brightness>")?;
        writeln!(w, "  <dither-pattern>I9</dither-pattern>")?; // Solid fill
        writeln!(w, "  <line-style/>")?;
        writeln!(w, "  <valid>true</valid>")?;
        writeln!(w, "  <visible>true</visible>")?;
        writeln!(w, "  <transparent>false</transparent>")?;
        writeln!(w, "  <width>1</width>")?;
        writeln!(w, "  <marked>false</marked>")?;
        writeln!(w, "  <xfill>false</xfill>")?;
        writeln!(w, "  <animation>0</animation>")?;
        writeln!(w, "  <name>L_{}</name>", mat_name)?;
        writeln!(w, "  <source>*/*@*</source>")?; // Match all layers with this name
        writeln!(w, " </properties>")?;
    }

    writeln!(w, "</layer-properties>")?;

    println!("   ✅ LYP: {}", path.display());
    println!("      ℹ️  Load this file in KLayout: File -> Load Layer Properties");
    Ok(())
}
