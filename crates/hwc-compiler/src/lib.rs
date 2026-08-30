//! HardwareScript v0.3.0 Compiler Core (`hwc-compiler`)
//!
//! Provides the Compile-Time Evaluation Engine (`hwc-eval`), Module Resolver,
//! Symbol Table, and Native Emitter bridges.
//!
//! The v0.3.0 compilation pipeline (lowering AST → `HardwareSpace`) lives in the
//! [`pipeline`] module, which has been extracted from this file to keep it readable.

use compact_str::CompactString;

pub mod embedded_stdlib;
pub mod error_codes;
pub mod eval;
pub mod ir;
pub mod module_resolver;
pub mod pipeline;
pub mod prelude;
pub mod span_utils;
pub mod symbol_table;

pub use eval::{
    eval_expression_bytecode, eval_expression_str, run_script, ControlFlow, DeterministicGuard,
    EscapeEnvelope, EvalError, EvaluationContext, Evaluator, MemoryEmitter, MeasurementValue,
    PhysicalDimension, PhysicalValue, SpaceEmitter, UnitDimension, Value,
};
pub use hwc_diagnostics::{DiagnosticCollector, ErrorFingerprint};
pub use module_resolver::{ModuleResolver, ResolverError};
pub use pipeline::{program_to_space, program_to_spaces_with_lockfile, PipelineError};
pub use prelude::{Prelude, PreludeError};
pub use symbol_table::{Definition, SymbolError, SymbolTable};

#[derive(Debug, Clone)]
pub struct CompilationMetadata {
    pub source_file: CompactString,
    pub space_count: usize,
    pub warnings: Vec<CompactString>,
}

/// Helper function to evaluate an entire parsed program with an evaluation context
pub fn evaluate_program(
    program: &hwc_parser::Program,
    ctx: &mut EvaluationContext,
) -> Result<(), EvalError> {
    let mut evaluator = Evaluator::new(ctx);
    evaluator.eval_program(program)
}
