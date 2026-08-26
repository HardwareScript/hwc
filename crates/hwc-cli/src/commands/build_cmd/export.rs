use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_export::netlist::types::PhysicalNetlist;
use hwc_export::{CompiledOutput, ExportFormat, Exporter};
use hwc_parser::{Program, TopLevelItem};
use miette::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Parameters for export operations
pub struct ExportParams<'a> {
    pub space: HardwareSpace,
    pub symbol_table: SymbolTable,
    pub ast: &'a Program,
    pub physical_netlist: Option<PhysicalNetlist>,
    pub output_dir: &'a PathBuf,
    pub formats: &'a [ExportFormat],
    pub start_time: Instant,
    pub unit_registry: hwc_types::UnitRegistry,
}

/// Export all requested formats plus auto-export utilities
pub fn export_all(params: ExportParams) -> Result<()> {
    let ExportParams {
        space,
        symbol_table,
        ast,
        physical_netlist,
        output_dir,
        formats,
        start_time,
        unit_registry,
    } = params;

    // Extract space definition for compiled output
    let space_def = ast.items.iter().find_map(|item| {
        if let TopLevelItem::Space(s) = item {
            if s.name.as_str() == space.name.as_str() {
                Some(s)
            } else {
                None
            }
        } else {
            None
        }
    }).or_else(|| {
        ast.items.iter().find_map(|item| {
            if let TopLevelItem::Space(s) = item {
                Some(s)
            } else {
                None
            }
        })
    });

    let compiled = CompiledOutput {
        space,
        symbol_table,
        space_def: space_def.cloned(),
        physical_netlist,
        unit_registry,
    };

    println!(
        "[{:>8.2}ms] About to create exporter...",
        start_time.elapsed().as_secs_f64() * 1000.0
    );
    let exporter = Exporter::new();

    for format in formats.iter() {
        let format_start = Instant::now();
        exporter
            .export(&compiled, output_dir, *format)
            .map_err(|e| miette::miette!("Export failed: {}", e))?;
        println!(
            "[{:>8.2}ms] Format {:?} exported in {:?}",
            start_time.elapsed().as_secs_f64() * 1000.0,
            format,
            format_start.elapsed()
        );
    }
    println!(
        "[{:>8.2}ms] All requested formats exported",
        start_time.elapsed().as_secs_f64() * 1000.0
    );

    // Auto-export utilities: BOM and Excellon
    auto_export_utilities(&exporter, &compiled, output_dir, formats)?;

    Ok(())
}

/// Auto-export BOM and Excellon if not already requested
fn auto_export_utilities(
    exporter: &Exporter,
    compiled: &CompiledOutput,
    space_output_dir: &Path,
    export_formats: &[ExportFormat],
) -> Result<()> {
    if !export_formats.contains(&ExportFormat::Bom) {
        let _start = Instant::now();
        exporter
            .export(compiled, space_output_dir, ExportFormat::Bom)
            .map_err(|e| miette::miette!("BOM export failed: {}", e))?;
    }

    if !export_formats.contains(&ExportFormat::Excellon) {
        let _start = Instant::now();
        exporter
            .export(compiled, space_output_dir, ExportFormat::Excellon)
            .map_err(|e| miette::miette!("Drill file export failed: {}", e))?;
    }

    Ok(())
}
