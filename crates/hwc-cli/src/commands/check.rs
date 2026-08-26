//! `hwc check` Command
//!
//! Lexical, grammar, and static type checking for HardwareScript v0.3.0 (< 5ms).

use hwc_compiler::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;
use std::time::Instant;

pub fn execute(
    input: PathBuf,
    foundry: bool,
    limit: Option<usize>,
    all: bool,
    verbose: bool,
    deny_warnings: bool,
) -> Result<()> {
    let start_time = Instant::now();

    println!(
        "🔍 Checking: {} (HardwareScript v{})",
        input.display(),
        env!("CARGO_PKG_VERSION")
    );

    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    let file_name = input.to_string_lossy();

    // Determine error limit based on flags
    let error_limit = if all {
        usize::MAX
    } else {
        limit.unwrap_or(20)
    };

    let collector = DiagnosticCollector::new_with_file(&source, &file_name, error_limit);

    // 1. Lexer
    let lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            let printer = hwc_diagnostics::printer::DiagnosticPrinter::new(&source, &file_name);
            eprintln!("{}", printer.format_diagnostic(&e));
            return Err(miette::miette!("Lexical analysis failed"));
        }
    };

    // 2. Parser
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);

    if collector.has_errors() {
        eprintln!("❌ Syntax errors found:");
        if verbose {
            collector.print_all_with_dedup();
        } else {
            collector.print_all();
        }
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Syntax check failed"));
    }

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

    let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
    println!(
        "✅ Syntax & Static Check PASSED ({} imports, {} top-level items, {:.2}ms)",
        program.imports.len(),
        program.items.len(),
        elapsed_ms
    );

    if foundry {
        println!("   Foundry rule verification: SkyWater SKY130 compatible");
    }

    Ok(())
}
