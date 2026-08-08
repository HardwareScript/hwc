use crate::commands::build_cmd::BuildConfig;
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
            if let hwc_parser::Definition::Space(space) = def {
                Some(space)
            } else {
                None
            }
        })
        .ok_or_else(|| miette::miette!("No space definition found in AST"))?;

    // Check for Artist Mode vs Professional Mode
    let is_artist_mode = space_def.implements_module.is_none();

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
        // **v0.2.2: Use device_instances from compiler instead of re-extracting**
        // The compiler already discovered devices during populate_device_instances(),
        // so we just convert that to PhysicalNetlist format for alignment/export.
        let extracted_netlist =
            hwc_compiler::ir::device_registry::device_instances_to_physical_netlist(
                space,
                Some(space_def),
                Some(symbol_table),
            );

        println!(
            "   ✅ Physical netlist extracted: {} devices",
            extracted_netlist.devices.len()
        );

        // Extract module definition for alignment validation
        let _module_def = ast.definitions.iter().find_map(|def| {
            if let hwc_parser::Definition::Module(module) = def {
                Some(module)
            } else {
                None
            }
        });

        // Run alignment validation
        // println!($3"[DEBUG] Running alignment validation...");
        let alignment_result = hwc_compiler::AlignmentValidator::validate(
            space_def,
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

        // Sprint 4.1.1: Run Physical Continuity Check (Layer 2 of Triple-Check Architecture)
        // Physical continuity validates actual copper paths before parameter extraction
        if !config.skip_physical_continuity {
            let (physics_substrate_layers, physics_route_segments) =
                super::validation::utils::convert_metadata_to_physics(space);

            let continuity_errors = super::validation::continuity::run_physical_continuity_check(
                space,
                &physics_substrate_layers,
                &physics_route_segments,
                config,
                start_time,
            )
            .map_err(|e| miette::miette!("Physical continuity validation error: {}", e))?;

            if !continuity_errors.is_empty() {
                println!(
                    "\n❌ PHYSICAL CONTINUITY VIOLATIONS - Cannot proceed to parameter validation:"
                );
                for error in &continuity_errors {
                    println!(
                        "   {} ({}): {}",
                        error.code,
                        error.message,
                        error.suggestion.as_ref().unwrap_or(&"No suggestion".into())
                    );
                }

                // Task 5.3: Respect --force-export flag
                if config.force_export {
                    println!("\n   ⚠️  --force-export: Continuing despite {} physical continuity violation(s)", 
                        continuity_errors.len());
                } else {
                    return Err(miette::miette!(
                        "Physical continuity validation failed with {} violation(s). Alignment Layer cannot validate fragmented nets.",
                        continuity_errors.len()
                    ));
                }
            } else {
                println!("   ✅ Physical continuity validation passed: All nets are physically continuous");
            }
        } else {
            println!(
                "\n   ⚠️  Physical continuity check skipped (--skip-physical-continuity flag)"
            );
        }

        // Sprint 4.1: Run Alignment Layer validation (Triple-Check Architecture)
        // Layer 1: Symbolic Alignment (device names, types)
        // Layer 2: Physical Continuity (already validated above)
        // Layer 3: Device Extraction (parameter validation)

        // TODO: Re-enable when AlignmentValidator is available in hwc_compiler
        /*
        if !config.skip_alignment {
            println!("\n🔍 Running Alignment Layer Validation (Triple-Check Architecture)...");
            // println!("[DEBUG] Starting Alignment check at {:.2}ms",
            //       start_time.elapsed().as_secs_f64() * 1000.0
            //   );

            let alignment_start = std::time::Instant::now();

            // Get module definition for logical graph extraction
            let module_def = module_def.ok_or_else(|| {
                miette::miette!("Module definition required for Alignment Layer validation")
            })?;

            // Create Alignment Validator
            let alignment_validator = hwc_compiler::AlignmentValidator::new(
                extracted_netlist.clone(),
                module_def,
                Some(symbol_table),
            );

            // Run validation
            let alignment_report = alignment_validator.validate();

            let alignment_duration = alignment_start.elapsed();
            println!(
                "[{:>8.2}ms] Alignment validation completed in {:.2}ms",
                start_time.elapsed().as_secs_f64() * 1000.0,
                alignment_duration.as_secs_f64() * 1000.0
            );

            // Print report
            if alignment_report.passed {
                println!("   ✅ ALIGNMENT PASSED - Layout implements module correctly");
                println!(
                    "      Devices: {} physical == {} logical",
                    alignment_report.physical_device_count, alignment_report.logical_device_count
                );
                println!(
                    "      Nets: {} physical == {} logical",
                    alignment_report.physical_net_count, alignment_report.logical_net_count
                );
            } else {
                println!(
                    "   ❌ ALIGNMENT FAILED - {} violation(s) found\n",
                    alignment_report.violations.len()
                );
                println!("{}", alignment_report);
                return Err(miette::miette!(
                    "Alignment validation failed with {} violation(s)",
                    alignment_report.violations.len()
                ));
            }
        } else {
            println!("   ⚠️  Alignment validation skipped (--skip-alignment flag)");
        }
        */

        // Task 4.3: Run Bulk Connection Validation
        if !config.skip_bulk_validation {
            println!("\nℹ️  Bulk connection validation skipped");
        }

        Ok(Some(extracted_netlist))
    }
}
