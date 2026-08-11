//! GLB (GL Transmission Format Binary) export for 3D visualization
//!
//! GLB is the binary version of glTF, widely supported by 3D viewers and game engines.

use crate::scene_graph::SceneGraph;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_parser::SpaceDefinition;
use std::path::Path;

pub fn export(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    export_with_space_def(space, symbol_table, output_dir, None)
}

pub fn export_with_space_def(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &Path,
    space_def: Option<&SpaceDefinition>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = output_dir.join("board.glb");

    // Build scene graph
    let mut scene = SceneGraph::new();
    scene.add_materials(symbol_table)?;

    // Get profile from space definition
    let profile = space_def
        .and_then(|sd| sd.profile.as_ref())
        .and_then(|profile_name| symbol_table.get_profile(profile_name.as_str()).ok());

    scene.add_substrate(space, profile, symbol_table);
    scene.add_traces(space);

    // Add components from HardwareSpace
    scene.add_components_from_space(space, symbol_table)?;

    // Export to GLB format
    let glb_data = scene.export_glb();

    std::fs::write(&path, glb_data)?;
    println!("   ✅ GLB: {}", path.display());

    Ok(())
}
