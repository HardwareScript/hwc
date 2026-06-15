// Modular build command structure
// This module orchestrates the hardware compilation pipeline

mod alignment;
mod compilation;
mod config;
mod error;
mod export;
mod lockfile;
mod parsing;
mod source_context;
mod validation;

pub use error::BuildError;

use compact_str::CompactString;
pub use config::BuildConfig;
use miette::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;


/// Main build execution entry point
pub fn execute(
    input: PathBuf,
    output: PathBuf,
    formats: Vec<CompactString>,
    config: BuildConfig,
) -> Result<()> {
    let start_time = Instant::now();
    println!("🔥 hwc COMPILER v0.1.6 (Syntax Unification)");
    println!("==================================================\n");

    // Resolve output directory relative to input file location
    let output_dir = resolve_output_dir(&input, &output)?;

    if config.verbose {
        print_configuration(&config, &output_dir);
    }

    // Parse export formats
    let export_formats = parsing::parse_formats(&formats)?;
    if config.verbose {
        // println!($3"[DEBUG] Export formats: {:?}\n", export_formats);
    }

    // Compile source to AST and symbol table
    let compilation_result = compilation::compile_source(&input, &config, start_time)?;

    // Transform AST to all HardwareSpaces
    let spaces_result = hwc_compiler::program_to_spaces(
        &compilation_result.ast,
        &compilation_result.symbol_table,
        &compilation_result.collector,
    );

    // Print any diagnostics
    if compilation_result.collector.has_any() {
        if config.verbose {
            compilation_result.collector.print_all_with_dedup();
        } else {
            compilation_result.collector.print_all();
        }
    }

    let mut spaces = match spaces_result {
        Ok(s) => s,
        Err(e) => {
            let file_name = input.to_string_lossy();
            let printer = hwc_diagnostics::printer::DiagnosticPrinter::new(
                &compilation_result.source,
                &file_name,
            );
            eprintln!("{}", printer.format_diagnostic(&e));
            return Err(miette::miette!(""));
        }
    };

    // Filter spaces by --space flag if provided
    if let Some(ref filter_name) = config.space {
        spaces.retain(|name, _| name.as_str() == filter_name.as_str());
        if spaces.is_empty() {
            println!("⚠️  No space named '{}' found in the source file", filter_name);
            return Ok(());
        }
    }

    println!(
        "[{:>8.2}ms] Found {} space(s) to build",
        start_time.elapsed().as_secs_f64() * 1000.0,
        spaces.len()
    );

    // Build each space
    for (space_name, mut space) in spaces {
        println!("\n── Building space: {} ──", space_name);

        println!(
            "[{:>8.2}ms] HardwareSpace created: {} ({}x{}x{})",
            start_time.elapsed().as_secs_f64() * 1000.0,
            space.name,
            space.grid.x_cols,
            space.grid.y_rows,
            space.grid.z_layers
        );

        // Run alignment validation
        let physical_netlist = alignment::validate_alignment(
            &compilation_result.ast,
            &mut space,
            &compilation_result.symbol_table,
            &config,
            start_time,
        )?;

        // Create space-specific output directory
        let space_output_dir = output_dir.join(&space.name);
        std::fs::create_dir_all(&space_output_dir)
            .map_err(|e| miette::miette!("Failed to create space output directory: {}", e))?;

        if config.verbose {
            println!("📁 Output directory: {}", space_output_dir.display());
        }

        // Handle route lockfile
        lockfile::handle_lockfile(&input, &space, &config, start_time)?;

        // Run validation checks
        let is_artist_mode = physical_netlist.is_none();
        let validation_result = validation::run_validation_checks(&space, &config, is_artist_mode, start_time)?;

        // Commit gate
        if !validation_result.passed && !is_artist_mode {
            if config.force_export {
                println!("\n⚠️  --force-export: Overriding Commit Gate despite {} violation(s)",
                    validation_result.violation_count);
                println!("   ⚠️  WARNING: Exporting design with known physical integrity issues");
            } else {
                return Err(miette::Report::new(BuildError::from_validation_failures(
                    &validation_result.violations
                )));
            }
        }

        if !validation_result.passed && is_artist_mode {
            println!("\n⚠️  Artist Mode: Exporting despite {} validation warning(s)",
                validation_result.violation_count);
        }

        // Realize analytic routes
        if !space.analytic_routes.is_empty() {
            space.realize_analytic_routes();
        }

        // Export all formats
        export::export_all(export::ExportParams {
            space,
            symbol_table: compilation_result.symbol_table.clone(),
            ast: &compilation_result.ast,
            physical_netlist,
            output_dir: &space_output_dir,
            formats: &export_formats,
            start_time,
        })?;
    }

    // Success message
    println!("    Finished build in {:.2}s", start_time.elapsed().as_secs_f64());

    Ok(())
}

/// Resolve output directory relative to input file location
fn resolve_output_dir(input: &Path, output: &Path) -> Result<PathBuf> {
    if output.is_absolute() {
        Ok(output.to_path_buf())
    } else {
        let input_dir = input
            .parent()
            .ok_or_else(|| miette::miette!("Input file has no parent directory"))?;
        Ok(input_dir.join(output))
    }
}

/// Print build configuration
fn print_configuration(config: &BuildConfig, output_dir: &Path) {
    println!("Configuration:");
    println!(
        "  DRC: {}",
        if config.skip_drc {
            "skipped"
        } else {
            "enabled"
        }
    );
    println!(
        "  Physics: {}",
        if config.skip_physics {
            "skipped"
        } else {
            "enabled"
        }
    );
    println!(
        "  Connectivity Check: {}",
        if config.skip_connectivity_check {
            "skipped"
        } else {
            "enabled"
        }
    );
    println!(
        "  Lockfile: {}",
        if config.no_lockfile {
            "disabled"
        } else if config.force_reroute {
            "ignored (force reroute)"
        } else {
            "enabled"
        }
    );
    println!("  Output: {}", output_dir.display());
    println!();
}
