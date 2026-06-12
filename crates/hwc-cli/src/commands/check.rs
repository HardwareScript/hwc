use hwc_compiler::validator::Validator;
use hwc_compiler::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;

pub fn execute(
    input: PathBuf,
    foundry: bool,
    limit: Option<usize>,
    all: bool,
    verbose: bool,
    deny_warnings: bool,
) -> Result<()> {
    if foundry {
        println!(
            "🔍 Checking: {} (v0.1.6 syntax + foundry validation)",
            input.display()
        );
    } else {
        println!("🔍 Checking: {} (v0.1.6 syntax)", input.display());
    }

    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    // eprintln!($3"[DEBUG] File read complete, {} bytes", source.len());

    // Determine error limit based on flags
    let error_limit = if all {
        usize::MAX // Show all errors
    } else {
        limit.unwrap_or(20) // Default: 20 errors (professional standard)
    };

    // Create diagnostic collector with configured limit
    let collector = DiagnosticCollector::new_with_file(&source, &input.to_string_lossy(), error_limit);

    // eprintln!($3"[DEBUG] Starting lexer...");
    let lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().map_err(|e| {
        // Convert lexer error to miette report with source context
        miette::Report::new(e).with_source_code(source.clone())
    })?;
    // eprintln!($3"[DEBUG] Lexer complete, {} tokens", tokens.len());

    // eprintln!($3"[DEBUG] Starting parser...");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&collector);
    // eprintln!($3"[DEBUG] Parser complete");

    // Check for syntax errors
    if collector.has_errors() {
        eprintln!("❌ Syntax errors:");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());

        // Check if this looks like v0.1.5 syntax and provide migration hint
        eprintln!("\n💡 Hint: This file may use v0.1.5 syntax.");
        eprintln!("   v0.1.6 changes:");
        eprintln!("   - Remove 'define' keyword: component Name: (not define component \"Name\":)");
        eprintln!("   - Use bare identifiers (no quotes on type names)");
        eprintln!("   - Use single '=' for comparison (not '==')");
        eprintln!("   - Use lowercase 'reg' (not 'Reg')");
        eprintln!("\n   See: https://docs.hw-script.org/v0.1.6/migration\n");

        return Err(miette::miette!("Syntax errors found"));
    }

    // Check for warnings if --deny-warnings is set
    if deny_warnings && collector.warning_count() > 0 {
        eprintln!("❌ Warnings treated as errors (--deny-warnings):");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Warnings found with --deny-warnings"));
    }

    println!("✅ Syntax valid (v0.1.6)");

    // Build symbol table for semantic validation
    use hwc_compiler::{ModuleResolver, SymbolTable};
    let mut symbol_table = SymbolTable::new();
    let mut resolver = ModuleResolver::new()
        .map_err(|e| miette::miette!("Failed to initialize module resolver: {}", e))?;

    // Process imports first
    for import in &ast.imports {
        resolver
            .resolve_import(import, &input, &mut symbol_table)
            .map_err(|e| miette::miette!("Import resolution failed: {}", e))?;
    }

    // Register local definitions
    for definition in &ast.definitions {
        match definition {
            hwc_parser::Definition::Material(mat) => {
                symbol_table.register_material(&collector, mat.clone());
            }
            hwc_parser::Definition::Profile(profile) => {
                symbol_table.register_profile(&collector, profile.clone());
            }
            hwc_parser::Definition::Component(component) => {
                symbol_table.register_component(&collector, component.clone());
            }
            hwc_parser::Definition::Module(module) => {
                symbol_table.register_module(&collector, module.clone());
            }
            hwc_parser::Definition::Mechanical(mechanical) => {
                symbol_table.register_mechanical(&collector, mechanical.clone());
            }
            hwc_parser::Definition::Interface(interface) => {
                symbol_table.register_interface(&collector, interface.clone());
            }
            hwc_parser::Definition::Test(test) => {
                symbol_table.register_test(&collector, test.clone());
            }
            _ => {}
        }
    }

    // Check for registration errors
    if collector.has_errors() {
        eprintln!("❌ Registration errors:");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Registration errors found"));
    }

    // Try to compile to HardwareSpace for full validation
    match hwc_compiler::program_to_space(&ast, &symbol_table, &collector) {
        Ok(space) => {
            println!("✅ Semantic validation passed");
            println!(
                "   - Space: {} ({}x{}x{})",
                space.name, space.grid.x_cols, space.grid.y_rows, space.grid.z_layers
            );
            println!("   - Components: {}", space.netlist.stats().component_count);
            println!("   - Nets: {}", space.netlist.stats().net_count);

            // Run foundry validation if flag is set
            if foundry {
                println!("\n🏭 Running foundry validation...");
                run_foundry_validation(&ast, &input, &source, &collector, verbose, deny_warnings)?;
            }
        }
        Err(e) => {
            // If there's no space definition, that's OK for check command
            // We still want to validate logic blocks
            if e.to_string().contains("No space definition found") {
                println!("✅ Semantic validation passed (no space definition)");
                println!("   - Modules validated: ✓");

                // Run foundry validation if flag is set
                if foundry {
                    println!("\n🏭 Running foundry validation...");
                    run_foundry_validation(
                        &ast,
                        &input,
                        &source,
                        &collector,
                        verbose,
                        deny_warnings,
                    )?;
                }
            } else {
                eprintln!("❌ Semantic error:");
                // Propagate the error with source context for beautiful diagnostics
                return Err(miette::Report::new(e).with_source_code(source));
            }
        }
    }

    Ok(())
}

/// Run foundry validation (MPV - Minimum Physical Viability)
fn run_foundry_validation(
    ast: &hwc_parser::Program,
    source_file: &std::path::Path,
    _source: &str,
    collector: &DiagnosticCollector,
    verbose: bool,
    deny_warnings: bool,
) -> Result<()> {
    use hwc_compiler::{ModuleResolver, SymbolTable};

    // Build symbol table with imports resolved (so property merging happens)
    let mut symbol_table = SymbolTable::new();
    let mut resolver = ModuleResolver::new()
        .map_err(|e| miette::miette!("Failed to initialize module resolver: {}", e))?;

    // Process imports first (goes into HPM layer)
    for import in &ast.imports {
        resolver
            .resolve_import(import, source_file, &mut symbol_table)
            .map_err(|e| miette::miette!("Failed to resolve import: {}", e))?;
    }

    // Register local definitions (goes into Local layer, with property merging)
    for definition in &ast.definitions {
        if let hwc_parser::Definition::Material(mat) = definition {
            symbol_table.register_material(collector, mat.clone());
        }
    }

    // Check for registration errors
    if collector.has_errors() {
        eprintln!("❌ Registration errors:");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Registration errors found"));
    }

    // Now validate using the merged materials from the symbol table
    let validator = Validator::new();
    validator.validate_materials_mpv_from_symbol_table(collector, &symbol_table);

    // Check for validation errors
    if collector.has_errors() {
        eprintln!("❌ Foundry validation failed:");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());

        eprintln!("\n💡 Hint: Materials must define these properties for foundry validation:");
        eprintln!("   - resistivity (Ω·m)");
        eprintln!("   - thermal_conductivity (W/(m·K))");
        eprintln!("   - density (kg/m³)");
        eprintln!("   - melting_point (K)");
        eprintln!("   - max_current_density (A/m²)");
        return Err(miette::miette!("Foundry validation failed"));
    }

    // Check for warnings
    if collector.warning_count() > 0 {
        if deny_warnings {
            eprintln!("❌ Foundry validation failed (warnings treated as errors):");
            if verbose {
                collector.print_all_with_dedup();
            } else {
                collector.print_all();
            }
            eprintln!("\n{}", collector.summary());
            return Err(miette::miette!("Warnings found with --deny-warnings"));
        } else {
            println!("⚠️  Foundry validation passed with warnings:");
            if verbose {
                collector.print_all_with_dedup();
            } else {
                collector.print_all();
            }
        }
    } else {
        println!("✅ Foundry validation passed");
        println!("   - All materials have required physical properties");
    }

    Ok(())
}
