use hwc_compiler::{program_to_space, SymbolTable};
use hwc_parser::{Lexer, Parser};
use hwc_physics::PhysicsEngine;
use miette::Result;
use std::path::PathBuf;

/// Execute physics validation on a hardware design.
///
/// This command runs comprehensive physics validation including:
/// - Electrical analysis (voltage drop, ampacity)
/// - Thermal analysis (temperature rise, clustering)
/// - Electromagnetic analysis (impedance, crosstalk)
/// - Clearance validation (dielectric breakdown)
///
/// # Arguments
/// * `input` - Path to .hw source file
/// * `build_dir` - Path to build directory (currently unused, for future use)
/// * `verbose` - Enable detailed analysis output
/// * `parallel` - Use parallel validation (faster on multi-core systems)
///
/// # Returns
/// Ok if physics validation passes, error with detailed violations if validation fails
pub fn execute(input: PathBuf, _build_dir: PathBuf, verbose: bool, parallel: bool) -> Result<()> {
    println!("⚡ PHYSICS VALIDATION");
    println!("==================================================\n");

    if verbose {
        println!("📋 Configuration:");
        println!("  Input file: {}", input.display());
        println!(
            "  Parallel validation: {}",
            if parallel { "enabled" } else { "disabled" }
        );
        println!();
    }

    // Read source
    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    // Lex
    if verbose {
        println!("🔤 Lexing...");
    }
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| miette::miette!("Lexer error: {}", e))?;

    // Parse with diagnostic collector
    if verbose {
        println!("🌳 Parsing...");
    }
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

    // Transform AST to hardware space and build symbol table
    if verbose {
        println!("🏗️  Building hardware space and symbol table...");
    }
    let mut symbol_table = SymbolTable::new(ast.arena.clone());

    // Register materials from AST
    for def in &ast.definitions {
        if let hwc_parser::ast::Definition::Material(mat_id) = def {
            let m = &ast.arena.material_defs[*mat_id];
            symbol_table.register_material(&collector, m.clone());
        }
    }

    // Check for registration errors
    if collector.has_errors() {
        eprintln!("❌ Registration errors:");
        collector.print_all();
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Registration errors found"));
    }

    let unit_registry = hwc_types::UnitRegistry::new(vec![]);
    let _space = program_to_space(&ast, &symbol_table, &collector, &unit_registry)
        .map_err(|e| miette::miette!("Failed to create hardware space: {}", e))?;

    // Initialize physics engine
    if verbose {
        println!("🔬 Initializing physics engine...");
        println!("  ✓ Using Symbol Table for material properties");
        println!();
    }
    let engine = PhysicsEngine::new();

    // Run physics validation
    println!("🔍 Running physics validation...");
    let report = if parallel {
        if verbose {
            println!("  Using parallel validation (multi-threaded)");
        }
        engine.validate_design_parallel(&symbol_table, None)
    } else {
        if verbose {
            println!("  Using sequential validation (single-threaded)");
        }
        engine.validate_design(&symbol_table, None)
    };

    // Display results
    println!();
    println!("{}", report.format_report());

    // Detailed analysis if verbose
    if verbose && !report.is_valid() {
        println!("\n📊 DETAILED ANALYSIS");
        println!("==================================================");

        if !report.electrical_violations.is_empty() {
            println!("\n⚡ Electrical Violations:");
            for (i, violation) in report.electrical_violations.iter().enumerate() {
                println!("  {}. {:?}", i + 1, violation);
            }
        }

        if !report.thermal_violations.is_empty() {
            println!("\n🔥 Thermal Violations:");
            for (i, violation) in report.thermal_violations.iter().enumerate() {
                println!("  {}. {:?}", i + 1, violation);
            }
        }

        if !report.em_violations.is_empty() {
            println!("\n📡 Electromagnetic Violations:");
            for (i, violation) in report.em_violations.iter().enumerate() {
                println!("  {}. {:?}", i + 1, violation);
            }
        }

        if !report.clearance_violations.is_empty() {
            println!("\n⚠️  Clearance Violations:");
            for (i, violation) in report.clearance_violations.iter().enumerate() {
                println!("  {}. {:?}", i + 1, violation);
            }
        }

        // Convert to error codes
        println!("\n🔢 ERROR CODES:");
        let errors = report.to_errors();
        for error in &errors {
            println!("  • {} - {}", error.code, error.message);
            if let Some(suggestion) = &error.suggestion {
                println!("    💡 {}", suggestion);
            }
        }
    }

    // Exit with error if validation failed
    if !report.is_valid() {
        return Err(miette::miette!(
            "Physics validation failed with {} violation(s)",
            report.total_violations()
        ));
    }

    println!("\n✅ PHYSICS VALIDATION COMPLETE!");
    Ok(())
}
