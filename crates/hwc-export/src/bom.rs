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
    _symbol_table: &SymbolTable,
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
            // Filter out internal routing anchors, layer polygons/pours, and vias from discrete orderable parts
            let is_stackup_layer = space
                .stackup_layers
                .iter()
                .any(|l| l.name == component.component_type);

            if is_stackup_layer
                || component.component_type.starts_with("Pour(")
                || component.component_type.starts_with("Contact(")
                || component.component_type.eq_ignore_ascii_case("via")
                || component.component_type == "Anchor"
            {
                continue;
            }

            let value = "";
            let package = "";
            let manufacturer = "";
            let part_number = "";

            let description = "";

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
        
        
        // Refinement 2: Get layer name from stackup using explicit layer_name
        let layer_name = if let Some(st) = space.stackup_layers.iter().find(|l| l.name == pour.layer_name) {
            format!("{} (z:{}nm)", st.name, st.z_bottom)
        } else {
            resolve_layer_name(&space.stackup_layers, pour.z_bottom_nm, &pour.name)?
        };

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

    // v0.2.2: Include routed traces from analytic_routes
    // Sort traces by net name for deterministic BOM output
    let mut sorted_traces: Vec<_> = space.analytic_routes.iter().collect();
    sorted_traces.sort_by(|a, b| a.net_name.as_str().cmp(b.net_name.as_str()));
    
    for trace in sorted_traces {
        let total_segs = trace.segments.len();
        if total_segs == 0 {
            continue;
        }

        // Calculate total trace length with boundary adjustment for electrical wire span
        // The segments span center-to-center for physical connectivity, but for BOM/costing
        // we calculate the actual interconnect length (excluding pad/contact bodies)
        let mut total_length_nm: i64 = 0;
        
        for (seg_num, segment) in trace.segments.iter().enumerate() {
            let mut dx = (segment.end.x - segment.start.x) as f64;
            let mut dy = (segment.end.y - segment.start.y) as f64;
            let dz = (segment.end.z - segment.start.z) as f64;

            // For first segment: subtract half of start pour dimension
            if seg_num == 0 {
                let start_point = (segment.start.x as f64, segment.start.y as f64);
                if let Some(pour) = space.pours.iter().find(|p| {
                    if p.layer_name != trace.layer_name {
                        return false;
                    }
                    if let Some(ref bbox) = p.bbox {
                        let px = start_point.0;
                        let py = start_point.1;
                        px >= bbox.min.x as f64 && px <= bbox.max.x as f64 &&
                        py >= bbox.min.y as f64 && py <= bbox.max.y as f64
                    } else {
                        false
                    }
                }) {
                    if let Some(bbox) = &pour.bbox {
                        let half_dim = if dx.abs() > dy.abs() {
                            ((bbox.max.x - bbox.min.x) as f64) / 2.0
                        } else {
                            ((bbox.max.y - bbox.min.y) as f64) / 2.0
                        };
                        if dx.abs() > dy.abs() {
                            dx -= half_dim * dx.signum();
                        } else {
                            dy -= half_dim * dy.signum();
                        }
                    }
                }
            }

            // For last segment: subtract half of end pour dimension
            // Only apply if this is not also the first segment (avoid double-subtracting on single-segment routes)
            if seg_num == total_segs - 1 && seg_num != 0 {
                let end_point = (segment.end.x as f64, segment.end.y as f64);
                if let Some(pour) = space.pours.iter().find(|p| {
                    if p.layer_name != trace.layer_name {
                        return false;
                    }
                    if let Some(ref bbox) = p.bbox {
                        let px = end_point.0;
                        let py = end_point.1;
                        px >= bbox.min.x as f64 && px <= bbox.max.x as f64 &&
                        py >= bbox.min.y as f64 && py <= bbox.max.y as f64
                    } else {
                        false
                    }
                }) {
                    if let Some(bbox) = &pour.bbox {
                        let half_dim = if dx.abs() > dy.abs() {
                            ((bbox.max.x - bbox.min.x) as f64) / 2.0
                        } else {
                            ((bbox.max.y - bbox.min.y) as f64) / 2.0
                        };
                        if dx.abs() > dy.abs() {
                            dx -= half_dim * dx.signum();
                        } else {
                            dy -= half_dim * dy.signum();
                        }
                    }
                }
            }

            // For single-segment routes (seg_num == 0 and seg_num == total_segs - 1)
            // we need to subtract the end pour as well
            if total_segs == 1 {
                let end_point = (segment.end.x as f64, segment.end.y as f64);
                if let Some(pour) = space.pours.iter().find(|p| {
                    if p.layer_name != trace.layer_name {
                        return false;
                    }
                    if let Some(ref bbox) = p.bbox {
                        let px = end_point.0;
                        let py = end_point.1;
                        px >= bbox.min.x as f64 && px <= bbox.max.x as f64 &&
                        py >= bbox.min.y as f64 && py <= bbox.max.y as f64
                    } else {
                        false
                    }
                }) {
                    if let Some(bbox) = &pour.bbox {
                        let half_dim = if dx.abs() > dy.abs() {
                            ((bbox.max.x - bbox.min.x) as f64) / 2.0
                        } else {
                            ((bbox.max.y - bbox.min.y) as f64) / 2.0
                        };
                        if dx.abs() > dy.abs() {
                            dx -= half_dim * dx.signum();
                        } else {
                            dy -= half_dim * dy.signum();
                        }
                    }
                }
            }

            let segment_length_nm = ((dx * dx + dy * dy + dz * dz).sqrt().abs()) as i64;
            total_length_nm += segment_length_nm;
        }
        
        if total_length_nm == 0 {
            continue; // Skip zero-length traces
        }

        // Calculate trace volume: length × width × thickness
        let width_nm = trace.cross_section.width_nm;
        let thickness_nm = trace.cross_section.thickness_nm;
        let volume_nm3 = (total_length_nm as u128 * width_nm as u128 * thickness_nm as u128)
            .min(u64::MAX as u128) as u64;
        
        // Calculate trace area: length × width
        let area_nm2 = total_length_nm * width_nm;

        if volume_nm3 == 0 || area_nm2 == 0 {
            continue; // Skip invalid traces
        }

        // Get material name
        let material_name = space
            .material_registry
            .get_name(trace.material)
            .unwrap_or("Unknown");

        // Generate trace reference name
        let trace_name = format!("Route_{}_on_{}", trace.net_name, trace.layer_name);

        // Get layer information
        let layer_info = format!("{} (trace)", trace.layer_name);

        bom.push_str(&format!(
            "{},Route,{},{},{},{}\n",
            trace_name, material_name, layer_info, area_nm2, volume_nm3
        ));

        // Accumulate material totals for routes
        let material_key = material_name.to_string();
        let entry = material_totals.entry(material_key).or_insert((0, 0, 0));
        entry.0 += area_nm2;
        entry.1 += volume_nm3;
        entry.2 += 1;
    }

    // v0.2.2: Include contacts/vias (Tungsten plugs) in material usage
    for contact in &space.contacts {
        // Calculate volume from bbox
        let volume_nm3: u64 = if let Some(bbox) = &contact.bbox {
            let width = (bbox.max.x - bbox.min.x).unsigned_abs() as u128;
            let height = (bbox.max.y - bbox.min.y).unsigned_abs() as u128;
            let depth = (bbox.max.z - bbox.min.z).unsigned_abs() as u128;
            (width * height * depth).min(u64::MAX as u128) as u64
        } else {
            0
        };

        // Skip zero-volume contacts
        if volume_nm3 == 0 {
            continue;
        }

        // Calculate area from bbox (top surface)
        let area_nm2: i64 = if let Some(bbox) = &contact.bbox {
            let width = (bbox.max.x - bbox.min.x).unsigned_abs();
            let height = (bbox.max.y - bbox.min.y).unsigned_abs();
            (width * height) as i64
        } else {
            0
        };

        if area_nm2 == 0 {
            continue;
        }

        // Get layer name from from_layer or Z start (lower connection point)
        let layer_name = if let Some(ref from) = contact.from_layer {
            if let Some(st) = space.stackup_layers.iter().find(|l| l.name.as_str() == from.as_str()) {
                format!("{} (z:{}nm)", st.name, st.z_bottom)
            } else {
                resolve_layer_name(&space.stackup_layers, contact.z_start_nm, &contact.name)?
            }
        } else {
            resolve_layer_name(&space.stackup_layers, contact.z_start_nm, &contact.name)?
        };

        bom.push_str(&format!(
            "{},Contact,{},{},{},{}\n",
            contact.name, contact.material_name, layer_name, area_nm2, volume_nm3
        ));

        // Accumulate material totals
        let material_key = contact.material_name.to_string();
        let entry = material_totals.entry(material_key).or_insert((0, 0, 0));
        entry.0 += area_nm2;
        entry.1 += volume_nm3;
        entry.2 += 1;
    }

    // Refinement 3: Add aggregated material totals
    if !material_totals.is_empty() {
        bom.push_str("\n# AGGREGATED MATERIAL TOTALS (Foundry Fabrication Summary)\n");
        bom.push_str("Material,Total_Area_nm2,Total_Volume_nm3,Layer_Count,Coverage_Percentage\n");

        // Calculate die area for coverage percentage
        let die_area_nm2 = space.dimensions.width_nm * space.dimensions.height_nm;

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

    let route_count = space.analytic_routes.len();
    let physical_material_count = physical_pours.len() + route_count + space.contacts.len();
    println!(
        "   ✅ BOM: {} ({} discrete items, {} material items: {} pours, {} routes, {} contacts)",
        path.display(),
        discrete_count,
        physical_material_count,
        physical_pours.len(),
        route_count,
        space.contacts.len()
    );

    Ok(())
}

/// Resolve layer name from Z coordinate, prioritizing routable layers at boundaries.
///
/// **BOM Layer Resolution Rules (v0.2.2 - External Audit Fix):**
/// 1. If the Z coordinate matches exactly at a boundary between dielectric and routable layer,
///    prioritize the routable layer (e.g., li1, metal1) over the dielectric (ild0, ild1).
/// 2. Otherwise, return the layer whose Z range contains the coordinate.
/// 3. This fixes the "Contact_A_LI reports ild0 instead of li1" bug.
/// 4. Returns an error if no matching layer is found (no silent failures).
fn resolve_layer_name(
    stackup: &[hwc_engine::space::StackupLayer],
    z_nm: i64,
    entity_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // First pass: Look for a routable layer starting exactly at this Z
    if let Some(layer) = stackup
        .iter()
        .find(|layer| layer.is_routable && layer.z_bottom == z_nm)
    {
        return Ok(format!("{} (z:{}nm)", layer.name, z_nm));
    }

    // Second pass: Look for any layer containing this Z (inclusive on both ends)
    if let Some(layer) = stackup.iter().find(|layer| {
        let layer_z_min = layer.z_bottom;
        let layer_z_max = layer.z_top;
        z_nm >= layer_z_min && z_nm <= layer_z_max
    }) {
        return Ok(format!("{} (z:{}nm)", layer.name, z_nm));
    }

    // Error: No matching layer found - this indicates a serious stackup configuration issue
    Err(format!(
        "BOM Export Error: Entity '{}' at Z={}nm does not match any layer in stackup. \
        Available layers: {}",
        entity_name,
        z_nm,
        stackup
            .iter()
            .map(|l| format!("{} ({}nm-{}nm)", l.name, l.z_bottom, l.z_top))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .into())
}
