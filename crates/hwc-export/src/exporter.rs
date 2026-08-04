use crate::{bom, dxf, excellon, glb, netlist};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use std::path::Path;

/// Export formats supported by Hardware Script
///
/// ## Professional Three (v0.1.6+)
/// The unified output strategy focuses on 3 universal formats:
/// - **GLB**: Visual Truth - 3D visualization with PBR materials, colors, transparency
/// - **DXF**: Physical Truth - Universal 2D layout for both Silicon and PCB (True Color + Transparency)
/// - **Netlist**: Electrical Truth - SPICE netlist for simulation
///
/// ## Why These Three?
/// - **GLB**: Industry standard for 3D visualization, opens everywhere
/// - **DXF**: Universal CAD format that works for BOTH semiconductor (KLayout) and PCB (KiCad)
/// - **Netlist**: Standard SPICE format for electrical simulation
///
/// ## Utilities
/// - **BOM**: Bill of Materials (CSV)
/// - **Excellon**: Drill files for manufacturing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    // Professional Three (default)
    Glb,     // Visual Truth: 3D visualization with embedded PBR materials
    Dxf,     // Physical Truth: Universal 2D layout (Silicon + PCB)
    Netlist, // Electrical Truth: SPICE simulation netlist

    // Utilities (auto-generated with main outputs)
    Bom,      // Bill of Materials
    Excellon, // Drill file
}

impl ExportFormat {
    /// Returns the default "Professional Three" formats
    pub fn professional_three() -> Vec<Self> {
        vec![Self::Glb, Self::Dxf, Self::Netlist]
    }

    /// Returns all supported formats
    pub fn all() -> Vec<Self> {
        vec![
            Self::Glb,
            Self::Dxf,
            Self::Netlist,
            Self::Bom,
            Self::Excellon,
        ]
    }
}

pub struct CompiledOutput {
    pub space: HardwareSpace,
    pub symbol_table: SymbolTable,
    pub space_def: Option<hwc_parser::SpaceDefinition>, // v0.1.6: For profile access
    pub physical_netlist: Option<hwc_compiler::alignment::PhysicalNetlist>, // v0.1.6: Extracted devices for netlist export
}

#[derive(Default)]
pub struct Exporter;

impl Exporter {
    pub fn new() -> Self {
        Self
    }

    pub fn export(
        &self,
        compiled: &CompiledOutput,
        output_dir: &Path,
        format: ExportFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match format {
            ExportFormat::Glb => glb::export_with_space_def(
                &compiled.space,
                &compiled.symbol_table,
                output_dir,
                compiled.space_def.as_ref(),
            ),
            ExportFormat::Dxf => dxf::export(
                &compiled.space,
                &compiled.symbol_table,
                output_dir,
                compiled.space_def.as_ref(),
            ),
            ExportFormat::Netlist => netlist::export(
                &compiled.space,
                &compiled.symbol_table,
                output_dir,
                compiled.physical_netlist.as_ref(),
                compiled.space_def.as_ref(),
            ),
            ExportFormat::Bom => bom::export(&compiled.space, &compiled.symbol_table, output_dir),
            ExportFormat::Excellon => excellon::export(&compiled.space, output_dir),
        }
    }
}
