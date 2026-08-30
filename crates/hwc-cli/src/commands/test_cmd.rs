//! `hwc test` Command
//!
//! Executes HardwareScript v0.3.0 layout testbenches, runs comptime assertions,
//! and verifies physical rules (< 100ms).

use hwc_compiler::eval::{EvaluationContext, Evaluator, MemoryEmitter};
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;
use std::time::Instant;

pub fn execute(input: PathBuf, verbose: bool) -> Result<()> {
    let start_time = Instant::now();

    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read test file: {}", e))?;

    let file_name = input.to_string_lossy();

    // 1. Tokenize
    let lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            let printer = hwc_diagnostics::printer::DiagnosticPrinter::new(&source, &file_name);
            eprintln!("{}", printer.format_diagnostic(&e));
            return Err(miette::miette!("Lexical analysis failed"));
        }
    };

    // 2. Parse
    let collector = hwc_compiler::DiagnosticCollector::new_with_file(&source, &file_name, 50);
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);

    if collector.has_errors() {
        collector.print_all();
        return Err(miette::miette!("Parsing test file failed"));
    }

    // 3. Comptime Test Execution via hwc-eval VM
    let emitter = Box::new(MemoryEmitter::new());
    let mut ctx = EvaluationContext::with_emitter(emitter);
    let mut evaluator = Evaluator::new(&mut ctx);

    println!("🧪 Running HardwareScript v{} Testbench: {}", env!("CARGO_PKG_VERSION"), input.display());

    evaluator
        .eval_program(&program)
        .map_err(|e| miette::miette!("Test assertion failed: {}", e))?;

    let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
    println!("\n✅ All tests PASSED ({:.2}ms)", elapsed_ms);

    if verbose {
        let mem = evaluator.ctx.emitter.as_any().downcast_ref::<MemoryEmitter>();
        if let Some(mem) = mem {
            println!(
                "   Physical components synthesized: {} polygons, {} contacts, {} routes",
                mem.polygons.len(),
                mem.contacts.len(),
                mem.routes.len(),
            );
        }
    }

    Ok(())
}
