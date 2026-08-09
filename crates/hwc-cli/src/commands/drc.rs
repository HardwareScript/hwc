use hwc_compiler::{program_to_space, SymbolTable};
use hwc_engine::constraint_manager::ConstraintRulebook;
use hwc_engine::design_rule_check::DesignRuleChecker;
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;

/// Execute design rule check on a hardware design.
///
/// This command runs only the DRC validation phase without rebuilding
/// the entire design. Useful for quick validation during iteration.
///
/// # Arguments
/// * `input` - Path to .hw source file
/// * `build_dir` - Path to build directory (currently unused, for future use)
///
/// # Returns
/// Ok if DRC passes, error with detailed violations if DRC fails
pub fn execute(input: PathBuf, _build_dir: PathBuf) -> Result<()> {
    println!("🔍 DESIGN RULE CHECK");
    println!("==================================================\n");

    // Read source
    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    // Lex
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| miette::miette!("Lexer error: {}", e))?;

    // Parse with diagnostic collector
    let collector =
        hwc_compiler::DiagnosticCollector::new_with_file(&source, &input.to_string_lossy(), 20);
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&collector);

    if collector.has_errors() {
        eprintln!("❌ Syntax errors:");
        collector.print_all();
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Syntax errors found"));
    }

    // Transform AST to hardware space (includes routing)
    let symbol_table = SymbolTable::new(ast.arena.clone());
    let unit_registry = hwc_types::UnitRegistry::new(vec![]);
    let space = program_to_space(&ast, &symbol_table, &collector, &unit_registry)
        .map_err(|e| miette::miette!("Failed to create hardware space: {}", e))?;

    println!("📊 Analyzing design...");

    // Build constraint rulebook from fabrication profile
    let mut constraint_rulebook = ConstraintRulebook::new(space.resolution_nm);

    if let Some(ref constraints) = space.fabrication_constraints {
        use hwc_engine::constraint_manager::{FabricationConstraints, StackupInfo};

        let stackup = constraints.stackup.as_ref().map(|s| StackupInfo {
            dielectric_height_nm: s.dielectric_height_nm,
            copper_thickness_nm: s.copper_thickness_nm,
            relative_permittivity: s.relative_permittivity,
            default_impedance_ohm: s.default_impedance_ohm,
        });

        let fab_constraints = FabricationConstraints {
            min_trace_width_nm: constraints.trace.min_width_nm,
            min_trace_spacing_nm: constraints.trace.min_spacing_nm,
            min_via_diameter_nm: constraints.via.min_diameter_nm,
            default_via_diameter_nm: constraints.via.default_diameter_nm,
            min_annular_ring_nm: constraints.via.min_annular_ring_nm,
            min_spacing_nm: constraints.via.min_spacing_nm,
            low_voltage_clearance_nm: constraints.clearance.low_voltage_nm,
            medium_voltage_clearance_nm: constraints.clearance.medium_voltage_nm,
            high_voltage_clearance_nm: constraints.clearance.high_voltage_nm,
            safety_factor: constraints.clearance.safety_factor,
            stackup,
            solder_mask_expansion_nm: constraints.solder_mask_expansion_nm,
            technology: constraints.technology,
        };

        constraint_rulebook.set_fabrication_constraints(fab_constraints);
    }

    // Run DRC validation
    println!("\n🔬 Running design rule checks...");
    let drc_checker = DesignRuleChecker::new();
    let report = drc_checker
        .check(&space, &constraint_rulebook)
        .map_err(|e| miette::miette!(e))?;

    // Display results
    println!("\n📋 DRC RESULTS");
    println!("==================================================");

    if report.is_valid() {
        println!("✅ All checks passed!");
        println!("\nChecks performed:");
        println!("  ✓ Clearance violations");
        println!("  ✓ Trace width violations");
        println!("  ✓ Thermal violations");
        println!("\n✅ DRC COMPLETE - Design is valid");
        Ok(())
    } else {
        println!("❌ {} violation(s) found\n", report.violations.len());

        for (i, violation) in report.violations.iter().enumerate() {
            println!("{}. {}", i + 1, violation);
        }

        Err(miette::miette!(
            "DRC failed with {} violation(s)",
            report.violations.len()
        ))
    }
}
