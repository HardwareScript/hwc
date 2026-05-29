use compact_str::CompactString;
use hwc_export::ExportFormat;
use miette::Result;

/// Parse export format strings into ExportFormat enum
pub fn parse_formats(formats: &[CompactString]) -> Result<Vec<ExportFormat>> {
    let mut result = Vec::new();

    for fmt in formats {
        match fmt.to_lowercase().as_str() {
            "all" | "professional" => {
                // Professional Three: GLB, DXF, Netlist
                result.extend(ExportFormat::professional_three());
                break;
            }
            // Professional Three
            "glb" => result.push(ExportFormat::Glb),
            "dxf" => result.push(ExportFormat::Dxf),
            "netlist" | "spice" | "sp" => result.push(ExportFormat::Netlist),
            // Utilities
            "bom" => result.push(ExportFormat::Bom),
            "excellon" | "drill" => result.push(ExportFormat::Excellon),
            _ => {
                return Err(miette::miette!(
                    "Unknown format: {}. Available formats: glb, dxf, netlist, bom, excellon, all",
                    fmt
                ));
            }
        }
    }

    // Default to Professional Three if no formats specified
    if result.is_empty() {
        result.extend(ExportFormat::professional_three());
    }

    Ok(result)
}
