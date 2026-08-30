use crate::commands::build_cmd::BuildConfig;
use hwc_compiler::SymbolTable;
use hwc_parser::{Lexer, Parser, Program, TopLevelItem};
use miette::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Result of the compilation phase
pub struct CompilationResult {
    pub ast: Program,
    pub symbol_table: SymbolTable,
    pub source: String,
    pub collector: hwc_compiler::DiagnosticCollector,
    prelude: hwc_compiler::Prelude,
}

impl CompilationResult {
    /// Build a UnitRegistry from the prelude units.
    pub fn unit_registry(&self) -> hwc_types::UnitRegistry {
        self.prelude.build_unit_registry()
    }
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
        "[{:>8.2}ms] Parser complete ({} imports, {} items)",
        start_time.elapsed().as_secs_f64() * 1000.0,
        ast.imports.len(),
        ast.items.len()
    );

    // Build symbol table
    let (symbol_table, prelude) = build_symbol_table(&ast, input, &collector, config, start_time)?;

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
        prelude,
    })
}

/// Build symbol table with imports and definitions
fn build_symbol_table(
    ast: &Program,
    input: &Path,
    collector: &hwc_compiler::DiagnosticCollector,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<(SymbolTable, hwc_compiler::Prelude)> {
    // v0.3.0: SymbolTable owns its own arena; items are pushed in via register_* calls.
    let mut symbol_table = SymbolTable::default();

    // Load prelude: `units` → UnitInfo values for UnitRegistry; `constants` → f64 math values.
    // Neither type maps to SymbolTable registration in v0.3.0 — the comptime evaluator
    // injects physical units and constants directly into the EvaluationContext scope.
    let prelude = hwc_compiler::Prelude::load()
        .map_err(|e| miette::miette!("Failed to load prelude: {}", e))?;

    let mut resolver = hwc_compiler::ModuleResolver::new()
        .map_err(|e| miette::miette!("Failed to initialize module resolver: {}", e))?;

    // Process imports
    for import in &ast.imports {
        if let Err(e) = resolver.resolve_import(import, input, &mut symbol_table) {
            let e_with_src = e.with_source(
                collector.source.to_string(),
                collector.file_name.to_string(),
            );
            collector.report(e_with_src);
        }
    }

    // Check for import errors before continuing
    if collector.has_errors() {
        collector.print_all();
        return Err(miette::miette!("Import resolution failed"));
    }

    // Register local top-level items.
    // v0.3.0: `Program::items` is a flat `Vec<TopLevelItem>` — each variant carries
    // its inline data. No arena indirection is needed from the call site.
    for item in &ast.items {
        match item {
            TopLevelItem::Function(f) => {
                symbol_table.register_function(collector, f.clone());
            }
            TopLevelItem::Struct(s) => {
                symbol_table.register_struct(collector, s.clone());
            }
            TopLevelItem::Enum(e) => {
                symbol_table.register_enum(collector, e.clone());
            }
            TopLevelItem::Const(c) => {
                // Register constant declarations
                // For now, treat them as zero-parameter functions that return their value
                symbol_table.register_function(collector, hwc_parser::FunctionDecl {
                    is_exported: c.is_exported,
                    name: c.name.clone(),
                    parameters: vec![],
                    return_type: c.type_annotation.clone(),
                    body: hwc_parser::Block {
                        statements: vec![],
                        tail_expr: None,
                        span: c.span,
                    },
                    span: c.span,
                });
            }
            TopLevelItem::Export(_) => {
                // Export declarations are re-exports, they don't register new symbols
                // Skip them during registration phase
            }
            TopLevelItem::Space(s) => {
                symbol_table.register_space(collector, s.clone());
            }
            TopLevelItem::Module(m) => {
                symbol_table.register_module(collector, m.clone());
            }
            TopLevelItem::Material(m) => {
                symbol_table.register_material(collector, m.clone());
            }
            TopLevelItem::Profile(p) => {
                symbol_table.register_profile(collector, p.clone());
            }
            TopLevelItem::Device(d) => {
                symbol_table.register_device(collector, d.clone());
            }
            TopLevelItem::Test(t) => {
                symbol_table.register_test(collector, t.clone());
            }
            TopLevelItem::Statement(_) => {
                // Top-level script statements are not physical design symbols;
                // they are handled by `hwc run`, not `hwc build`.
            }
            TopLevelItem::Impl(impl_decl) => {
                // Register each method as a qualified `StructName::method_name` function.
                // The VM's CallMethod opcode looks up methods using exactly this key
                // (see vm.rs: `format!("{}::{}", struct_name, method_name)`).
                let struct_name = impl_decl.target.name.as_str();
                for method in &impl_decl.methods {
                    let qualified_name = format!("{}::{}", struct_name, method.name.name.as_str());
                    let mut qualified_method = method.clone();
                    qualified_method.name.name = qualified_name.as_str().into();
                    symbol_table.register_function(collector, qualified_method);
                }
            }
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

    Ok((symbol_table, prelude))
}
