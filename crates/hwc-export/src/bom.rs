//! Bill of Materials (BOM) Generation
//!
//! Generates CSV files listing all components with their metadata for manufacturing
//! and procurement. Loads component metadata from Symbol Table.

use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use std::path::Path;

/// Generate Bill of Materials (BOM) from HardwareSpace
pub fn export(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = output_dir.join("bom.csv");

    let mut bom = String::new();
    bom.push_str("Reference,Type,Value,Package,Manufacturer,Part Number,Description,Quantity\n");

    // Add substrate wafer as first BOM item
    let _substrate_material = space
        .material_registry
        .get_name(space.substrate_material_id)
        .unwrap_or("Unknown");
    let (width_mm, height_mm, depth_mm) = space.dimensions.to_mm();
    bom.push_str(&format!(
        "Wafer,Substrate,{:.2}x{:.2}x{:.2}mm,,,,,1\n",
        width_mm, height_mm, depth_mm
    ));

    // v0.1.7: Filter out internal routing anchors and pours from the BOM.
    // Physically orderable components are discrete parts like Resistors, ICs, etc.
    // Pours (Copper, Silicon) are fabrication steps, not orderable items.

    // Add discrete components from netlist arena
    let mut discrete_count = 0;
    let component_count = space.netlist.component_count();
    for i in 0..component_count {
        let comp_id = hwc_engine::netlist::ComponentId::new(i as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
            // v0.1.7: Filter out internal routing anchors
            if component.component_type.starts_with("Pour(")
                || component.component_type.starts_with("Contact(")
                || component.component_type == "Via"
                || component.component_type == "Anchor"
            {
                continue;
            }

            // Try to get metadata from symbol table
            let metadata = symbol_table
                .get_component(&component.component_type)
                .ok()
                .and_then(|comp_def| comp_def.metadata.as_ref());

            let value = metadata
                .and_then(|m| m.value.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let package = metadata
                .and_then(|m| m.package.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let manufacturer = metadata
                .and_then(|m| m.manufacturer.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let part_number = metadata
                .and_then(|m| m.part_number.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let description = metadata
                .and_then(|m| m.description.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            bom.push_str(&format!(
                "{},{},{},{},{},{},{},1\n",
                component.name,
                component.component_type,
                value,
                package,
                manufacturer,
                part_number,
                description
            ));
            discrete_count += 1;
        }
    }

    // Add material usage section for ASIC/fabrication tracking
    bom.push_str("\n# MATERIAL USAGE (Fabrication)\n");
    bom.push_str("Reference,Type,Material,Layer_Z_nm,Area_nm2,Volume_nm3\n");
    
    // Track pours from space.pours metadata
    let material_count = space.pours.len();
    for pour in &space.pours {
        let layer_info = format!("z:{}", pour.z_bottom_nm);
        
        // Calculate volume if bbox available
        let volume_nm3 = if let Some(bbox) = &pour.bbox {
            let width = (bbox.max.x - bbox.min.x) as u64;
            let height = (bbox.max.y - bbox.min.y) as u64;
            let depth = (bbox.max.z - bbox.min.z) as u64;
            width * height * depth
        } else {
            0
        };

        bom.push_str(&format!(
            "{},Pour,{},{},{},{}\n",
            pour.name,
            pour.material_name,
            layer_info,
            pour.area_nm2,
            volume_nm3
        ));
    }

    std::fs::write(&path, bom)?;

    let _total_items = 1 + discrete_count + material_count; // substrate + discrete components + materials
    println!("   ✅ BOM: {} ({} discrete items, {} material items)", path.display(), discrete_count, material_count);

    Ok(())
}
