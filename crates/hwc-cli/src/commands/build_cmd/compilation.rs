use crate::commands::build_cmd::BuildConfig;
use hwc_compiler::SymbolTable;
use hwc_parser::{Lexer, Parser, Program};
use miette::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Result of the compilation phase
pub struct CompilationResult {
    pub ast: Program,
    pub symbol_table: SymbolTable,
    pub source: String,
    pub collector: hwc_compiler::DiagnosticCollector,
}

/// Compile source file to AST and symbol table
pub fn compile_source(
    input: &PathBuf,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<CompilationResult> {
    // Read source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    println!(
        "[{:>8.2}ms] Source file read successfully ({} bytes)",
        start_time.elapsed().as_secs_f64() * 1000.0,
        source.len()
    );

    let file_name = input.to_string_lossy();

    // Lex
    let lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            let printer = hwc_diagnostics::printer::DiagnosticPrinter::new(&source, &file_name);
            eprintln!("{}", printer.format_diagnostic(&e));
            return Err(miette::miette!(""));
        }
    };

    println!(
        "[{:>8.2}ms] Lexer complete ({} tokens)",
        start_time.elapsed().as_secs_f64() * 1000.0,
        tokens.len()
    );

    // Parse with diagnostic collector
    // Sprint 9 (Task 9.3): Default cap is 50, matching the Rust compiler model.
    // Users can override with --limit N (lower) or --all (unlimited).
    let error_limit = if config.all {
        usize::MAX
    } else {
        config.limit.unwrap_or(50)
    };

    let collector =
        hwc_compiler::DiagnosticCollector::new_with_file(&source, &file_name, error_limit);
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&collector);

    if collector.has_errors() {
        if config.verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!(""));
    }

    if config.deny_warnings && collector.warning_count() > 0 {
        eprintln!("❌ Warnings treated as errors (--deny-warnings):");
        if config.verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!(""));
    }

    println!(
        "[{:>8.2}ms] Parser complete ({} imports, {} definitions)",
        start_time.elapsed().as_secs_f64() * 1000.0,
        ast.imports.len(),
        ast.definitions.len()
    );

    // Build symbol table
    let symbol_table = build_symbol_table(&ast, input, &collector, config, start_time)?;

    // Print warnings if any
    if collector.warning_count() > 0 {
        if config.verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("{}\n", collector.summary());
    }

    Ok(CompilationResult {
        ast,
        symbol_table,
        source,
        collector,
    })
}

/// Build symbol table with imports and definitions
fn build_symbol_table(
    ast: &Program,
    input: &Path,
    collector: &hwc_compiler::DiagnosticCollector,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<SymbolTable> {
    let mut symbol_table = SymbolTable::new();

    // Load prelude
    let prelude = hwc_compiler::Prelude::load()
        .map_err(|e| miette::miette!("Failed to load prelude: {}", e))?;

    // Register prelude units and constants
    for unit in &prelude.units {
        symbol_table.register_prelude_unit(unit.clone());
    }
    for (name, value) in &prelude.constants {
        symbol_table.register_prelude_constant(name.clone(), *value);
    }

    let mut resolver = hwc_compiler::ModuleResolver::new()
        .map_err(|e| miette::miette!("Failed to initialize module resolver: {}", e))?;

    // Process imports
    for import in &ast.imports {
        if let Err(e) = resolver.resolve_import(import, input, &mut symbol_table) {
            // Add source code for better error display
            let e_with_src = e.with_source(
                collector.source.to_string(),
                collector.file_name.to_string()
            );
            collector.report(e_with_src);
        }
    }

    // Check for import errors before continuing
    if collector.has_errors() {
        collector.print_all();
        return Err(miette::miette!("Import resolution failed"));
    }

    // Register local definitions
    for definition in &ast.definitions {
        match definition {
            hwc_parser::Definition::Bridge(bridge) => {
                symbol_table.register_bridge(collector, bridge.clone());
            }
            hwc_parser::Definition::Unit(unit) => {
                symbol_table.register_unit(collector, unit.clone());
            }
            hwc_parser::Definition::Material(mat) => {
                symbol_table.register_material(collector, mat.clone());
            }
            hwc_parser::Definition::Profile(profile) => {
                symbol_table.register_profile(collector, (**profile).clone());
            }
            hwc_parser::Definition::Component(component) => {
                symbol_table.register_component(collector, component.clone());
            }
            hwc_parser::Definition::Module(module) => {
                symbol_table.register_module(collector, module.clone());
            }
            hwc_parser::Definition::Mechanical(mechanical) => {
                symbol_table.register_mechanical(collector, mechanical.clone());
            }
            hwc_parser::Definition::Interface(interface) => {
                symbol_table.register_interface(collector, interface.clone());
            }
            hwc_parser::Definition::Test(test) => {
                symbol_table.register_test(collector, test.clone());
            }
            hwc_parser::Definition::Shape(shape) => {
                symbol_table.register_shape(collector, shape.clone());
            }
            _ => {}
        }
    }

    // Check for registration errors
    if collector.has_errors() {
        if config.verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!(""));
    }

    if config.deny_warnings && collector.warning_count() > 0 {
        eprintln!("❌ Warnings treated as errors (--deny-warnings):");
        if config.verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!(""));
    }

    println!(
        "[{:>8.2}ms] Symbol table built",
        start_time.elapsed().as_secs_f64() * 1000.0
    );

    Ok(symbol_table)
}
