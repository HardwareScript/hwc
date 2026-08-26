//! `hwc eval` Command
//!
//! Executes HardwareScript v0.3.0 comptime functions, evaluates `println()` / `dbg()` diagnostics,
//! and runs the `hwc-eval` virtual machine without physical meshing (< 10ms).

use compact_str::CompactString;
use hwc_compiler::eval::{EvaluationContext, Evaluator, MemoryEmitter};
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;
use std::time::Instant;

pub fn execute(input: PathBuf, verbose: bool) -> Result<()> {
    let start_time = Instant::now();

    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read source file: {}", e))?;

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
        return Err(miette::miette!("Parsing failed"));
    }

    // 3. Comptime Evaluation via hwc-eval VM
    let emitter = Box::new(MemoryEmitter::new());
    let mut ctx = EvaluationContext::with_emitter(emitter);
    let mut evaluator = Evaluator::new(&mut ctx);

    evaluator
        .eval_program(&program)
        .map_err(|e| miette::miette!("Evaluation runtime error: {}", e))?;

    if verbose {
        let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
        println!("\n✅ Comptime evaluation completed in {:.2}ms", elapsed_ms);
        let mem = evaluator.ctx.emitter.as_any().downcast_ref::<MemoryEmitter>();
        if let Some(mem) = mem {
            println!(
                "   Emitted: {} polygons, {} contacts, {} devices, {} routes, {} nets",
                mem.polygons.len(),
                mem.contacts.len(),
                mem.devices.len(),
                mem.routes.len(),
                mem.nets.len(),
            );
        }
    }

    Ok(())
}
