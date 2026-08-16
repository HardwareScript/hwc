use crate::commands::build_cmd::{BuildConfig, BuildError};
use hwc_compiler::{alignment::PhysicalNetlist, SymbolTable};
use hwc_engine::HardwareSpace;
use hwc_parser::Program;
use miette::Result;
use std::time::Instant;

/// Run alignment validation (Artist vs Professional mode)
/// Returns Some(PhysicalNetlist) in Professional mode, None in Artist mode
pub fn validate_alignment(
    ast: &Program,
    space: &mut HardwareSpace,
    symbol_table: &SymbolTable,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Option<PhysicalNetlist>> {
    // Extract space definition from AST
    let space_def = ast
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(space_id) = def {
                Some(*space_id)
            } else {
                None
            }
        })
        .ok_or_else(|| miette::miette!("No space definition found in AST"))?;

    // Lookup the actual SpaceDefinition from arena
    let space_def_actual = &ast.arena.space_defs[space_def];

    // Check for Artist Mode vs Professional Mode
    let is_artist_mode = space_def_actual.implements_module.is_none();

    if is_artist_mode {
        println!("🎨 Artist Mode: No 'implements' clause - Alignment validation skipped");
        println!("   ℹ️  Building geometry without logic verification");
        println!(
            "[{:>8.2}ms] Artist Mode check complete",
            start_time.elapsed().as_secs_f64() * 1000.0
        );
        Ok(None)
    } else {
        println!("🔍 Professional Mode: Alignment validation enabled");
        // println!($3"[DEBUG] Starting device extraction at {:.2}ms",
        //      start_time.elapsed().as_secs_f64() * 1000.0
        //   );

        // **HANDSHAKE C: GEOMETRIC REALIZATION (Sprint 3.12 - Gap 3 Fix)**
        //
        // Realize analytic routes for geometric analysis.
        // This enables:
        // - Device extraction (silicon: copper-silicon contact detection)
        // - Parasitic extraction (both PCB and silicon)
        // - Physical verification (DRC, Alignment Layer)
        //
        // **Performance:** Called once at end (lazy realization pattern)
        // - 3 routes: ~0.01s (vs 13.44s if done during routing)
        // - Bulk operation with sparse chunk allocation
        // - Universal: works for both PCB and silicon designs
        if !space.analytic_routes.is_empty() {
            // println!($3"[DEBUG] Realizing {} analytic routes for geometric analysis...",
            //    space.analytic_routes.len()
            // );

            // println!($3"[DEBUG] Geometric realization complete in {:.6}s",
            //    realize_duration.as_secs_f64()
            //  );
        }

        // Extract physical netlist from geometry
        // **v0.2.2+: Use NEW registry-based DeviceExtractor with error[D03] detection**
        let module_def_id = ast.definitions.iter().find_map(|def| {
            if let hwc_parser::Definition::Module(module_id) = def {
                Some(*module_id)
            } else {
                None
            }
        });

        // Resolve ModuleDefId to &ModuleDefinition through the arena
        let module_def = module_def_id.map(|id| &ast.arena.module_defs[id]);

        let mut device_extractor = hwc_export::DeviceExtractor::new(
            space,
            symbol_table,
            &ast.arena,
            Some(space_def_actual),
        );

        let extracted_netlist = device_extractor
            .extract_devices_with_module(module_def)
            .map_err(|errors| {
                // Convert Vec<DeviceExtractionError> to BuildError
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| format!("{}", e))
                    .collect();
                BuildError::DeviceExtractionFailed {
                    message: format!("Device extraction failed:\n{}", error_messages.join("\n")),
                }
            })?;

        println!(
            "   ✅ Physical netlist extracted: {} devices",
            extracted_netlist.devices.len()
        );

        // Extract module definition for alignment validation
        // (already extracted above for device extraction)

        // Run alignment validation
        // println!($3"[DEBUG] Running alignment validation...");
        let alignment_result = hwc_compiler::AlignmentValidator::validate(
            space_def_actual,
            &extracted_netlist,
            symbol_table,
            space,
            config.tolerance,
            &ast.arena,
        )
        .map_err(|e| miette::miette!("Alignment validation error: {}", e))?;
        // println!($3"[DEBUG] Alignment validation complete at {:.2}ms",
        //      start_time.elapsed().as_secs_f64() * 1000.0
        //   );

        match &alignment_result {
            hwc_compiler::AlignmentResult::Skipped { reason } => {
                println!("   ⚠️  Unexpected: {}", reason);
            }
            hwc_compiler::AlignmentResult::Passed {
                physical_device_count,
                logical_device_count,
            } => {
                println!(
                    "   ✅ Physical netlist extracted: {} devices",
                    physical_device_count
                );
                println!(
                    "   ✅ Logical netlist synthesized: {} devices",
                    logical_device_count
                );
                println!("   ✅ Alignment validation passed: Layout matches schematic");
            }
            hwc_compiler::AlignmentResult::Failed { error } => {
                println!("   ❌ Alignment validation failed\n");
                eprintln!("❌ ALIGNMENT ERROR: {}", error);
                eprintln!("\nBuild failed. No exports generated.");
                eprintln!("Fix the alignment errors above and try again.");
                return Err(miette::miette!("Alignment validation failed"));
            }
        }



        Ok(Some(extracted_netlist))
    }
}
