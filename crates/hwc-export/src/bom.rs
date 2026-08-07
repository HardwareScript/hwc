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
    bom.push_str("Reference,Type,Material,Layer,Area_nm2,Volume_nm3\n");

    // Track pours from space.pours metadata
    // Filter out virtual/zero-volume entities (Air material, zero volume)
    let mut physical_pours = Vec::new();
    let mut material_totals: std::collections::HashMap<String, (i64, u64, usize)> =
        std::collections::HashMap::new();

    for pour in &space.pours {
        // Calculate volume if bbox available
        let volume_nm3: u64 = if let Some(bbox) = &pour.bbox {
            let width = (bbox.max.x - bbox.min.x).unsigned_abs() as u128;
            let height = (bbox.max.y - bbox.min.y).unsigned_abs() as u128;
            let depth = (bbox.max.z - bbox.min.z).unsigned_abs() as u128;
            (width * height * depth).min(u64::MAX as u128) as u64
        } else {
            0
        };

        // Refinement 1: Filter out zero-volume and virtual (Air) entities
        if volume_nm3 == 0 || pour.area_nm2 == 0 || pour.material_name == "Air" {
            continue;
        }

        // Refinement 2: Get layer name from stackup
        let layer_name = space
            .stackup_layers
            .iter()
            .find(|layer| {
                let layer_z_min = layer.z_bottom;
                let layer_z_max = layer.z_top;
                pour.z_bottom_nm >= layer_z_min && pour.z_bottom_nm <= layer_z_max
            })
            .map(|layer| format!("{} (z:{}nm)", layer.name, pour.z_bottom_nm))
            .unwrap_or_else(|| format!("z:{}nm", pour.z_bottom_nm));

        physical_pours.push((
            pour.name.to_string(),
            pour.material_name.to_string(),
            layer_name.clone(),
            pour.area_nm2,
            volume_nm3,
        ));

        // Refinement 3: Accumulate material totals
        let material_key = pour.material_name.to_string();
        let entry = material_totals.entry(material_key).or_insert((0, 0, 0));
        entry.0 += pour.area_nm2;
        entry.1 += volume_nm3;
        entry.2 += 1;
    }

    // Write physical pours
    for (name, material, layer, area, volume) in &physical_pours {
        bom.push_str(&format!(
            "{},Pour,{},{},{},{}\n",
            name, material, layer, area, volume
        ));
    }

    // Refinement 3: Add aggregated material totals
    if !material_totals.is_empty() {
        bom.push_str("\n# AGGREGATED MATERIAL TOTALS (Foundry Fabrication Summary)\n");
        bom.push_str("Material,Total_Area_nm2,Total_Volume_nm3,Layer_Count,Coverage_Percentage\n");

        // Calculate die area for coverage percentage
        let die_area_nm2 = space.dimensions.width_nm as i64 * space.dimensions.height_nm as i64;

        // Sort by material name for consistent output
        let mut materials: Vec<_> = material_totals.iter().collect();
        materials.sort_by_key(|(name, _)| name.as_str());

        for (material, (total_area, total_volume, layer_count)) in materials {
            let coverage_percentage = if die_area_nm2 > 0 {
                (*total_area as f64 / die_area_nm2 as f64) * 100.0
            } else {
                0.0
            };

            bom.push_str(&format!(
                "{},{},{},{},{:.1}%\n",
                material, total_area, total_volume, layer_count, coverage_percentage
            ));
        }
    }

    std::fs::write(&path, bom)?;

    let physical_material_count = physical_pours.len();
    println!(
        "   ✅ BOM: {} ({} discrete items, {} material items)",
        path.display(),
        discrete_count,
        physical_material_count
    );

    Ok(())
}
