//! `hwc run` Command
//!
//! Executes HardwareScript compute scripts (runs top-level code or `fn main()`),
//! streaming `println()`, `eprintln()`, and `dbg()` directly to stdout/stderr with
//! ZERO physical synthesis / GDSII / GLB overhead (< 2ms).

use compact_str::CompactString;
use hwc_compiler::eval::{EvaluationContext, MemoryEmitter};
use hwc_compiler::{ModuleResolver, SymbolTable};
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

pub fn execute(input: PathBuf, target_fn: Option<CompactString>, verbose: bool) -> Result<()> {
    let start_time = Instant::now();

    if !input.exists() {
        return Err(miette::miette!("File not found: {}", input.display()));
    }

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

    // 3. Resolve imports into SymbolTable (same pattern as build_cmd/compilation.rs)
    let mut symbol_table = SymbolTable::default();
    let mut resolver = ModuleResolver::new()
        .map_err(|e| miette::miette!("Module resolver error: {}", e))?;

    for import in &program.imports {
        if let Err(e) = resolver.resolve_import(import, &input, &mut symbol_table) {
            let e_with_src = e.with_source(source.clone(), file_name.to_string());
            collector.report(e_with_src);
        }
    }

    if collector.has_errors() {
        collector.print_all();
        return Err(miette::miette!("Import resolution failed"));
    }

    // 4. Load stdlib unit registry
    let unit_registry = hwc_stdlib::load_stdlib_registry()
        .unwrap_or_else(|_| hwc_types::UnitRegistry::new(vec![]));

    // 5. Build EvaluationContext from resolved symbol table
    let emitter = Box::new(MemoryEmitter::new());
    let mut ctx = EvaluationContext::with_emitter(emitter);
    ctx.unit_registry = Some(Arc::new(unit_registry));

    // Load imported functions, structs, and enums from symbol table arena
    for func_def in symbol_table.arena().function_defs.iter() {
        ctx.functions.insert(func_def.name.name.clone(), func_def.clone());
    }
    for struct_def in symbol_table.arena().struct_defs.iter() {
        ctx.structs.insert(struct_def.name.name.clone(), struct_def.clone());
    }
    for enum_def in symbol_table.arena().enum_defs.iter() {
        // Register enum as an EnumType value so the VM can construct variants
        let variants = enum_def
            .variants
            .iter()
            .map(|v| {
                (
                    v.name.clone(),
                    hwc_compiler::eval::Value::EnumVariant {
                        enum_name: enum_def.name.name.clone(),
                        variant_name: v.name.clone(),
                        payload: None,
                    },
                )
            })
            .collect::<rustc_hash::FxHashMap<_, _>>();
        let enum_val = hwc_compiler::eval::Value::EnumType {
            name: enum_def.name.name.clone(),
            variants: Arc::new(variants),
        };
        ctx.enum_types.insert(enum_def.name.name.clone(), enum_val);
    }

    // 6. Execute Script via Bytecode VM
    let target_str = target_fn.as_deref();
    hwc_compiler::run_script(&program, &mut ctx, target_str)
        .map_err(|e| miette::miette!("Runtime error: {}", e))?;

    if verbose {
        let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
        eprintln!("\n✅ Script execution finished in {:.2}ms", elapsed_ms);
    }

    Ok(())
}
