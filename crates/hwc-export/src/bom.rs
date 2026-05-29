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

    // Add pours (mask layers, interconnects, silicon regions)
    for pour in &space.pours {
        let area_mm2 = pour.area_nm2 as f64 / 1_000_000_000_000.0; // nm² to mm²

        // Calculate volume from area and layer thickness
        // Assuming voxel_size.z_nm is the layer thickness
        let thickness_nm = space.voxel_size.z_nm as f64;
        let volume_nm3 = pour.area_nm2 as f64 * thickness_nm;
        let volume_mm3 = volume_nm3 / 1_000_000_000_000_000_000.0; // nm³ to mm³

        let net_info = pour
            .net
            .as_ref()
            .map(|n| format!(" net:{}", n))
            .unwrap_or_default();

        // For pours, use volume instead of quantity
        bom.push_str(&format!(
            "{},Pour,{:.4}mm²{},Z {:.4}mm,,{},{:.6}mm³\n",
            pour.name,
            area_mm2,
            net_info,
            pour.z_bottom_nm as f64 / 1_000_000_000.0,
            pour.material_name,
            volume_mm3
        ));
    }

    // Add discrete components from netlist arena
    let component_count = space.netlist.component_count();
    for i in 0..component_count {
        let comp_id = hwc_engine::netlist::ComponentId::new(i as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
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
        }
    }

    std::fs::write(&path, bom)?;

    let total_items = 1 + space.pours.len() + component_count; // substrate + pours + components
    println!("   ✅ BOM: {} ({} items)", path.display(), total_items);

    Ok(())
}
