//! HardwareScript v0.3.0 Comptime Evaluation Engine (`hwc-eval`)
//!
//! Powered entirely by a Linear Bytecode Virtual Machine with static activation records.

pub mod builtins;
pub mod compiler;
pub mod context;
pub mod emitter;
pub mod escape_contract;
pub mod frame;
pub mod opcodes;
pub mod sandbox;
pub mod value;
pub mod vm;

pub use compiler::BytecodeCompiler;
pub use context::{Binding, EvalError, EvaluationContext, ScopeFrame};
pub use emitter::{
    ContactRecord, DeviceRecord, MemoryEmitter, PolygonRecord, RouteRecord, SpaceEmitter,
};
pub use escape_contract::EscapeEnvelope;
pub use frame::CallFrame;
pub use opcodes::{Chunk, ConstantIndex, JumpOffset, OpCode, Register};
pub use sandbox::{SandboxGuard, MAX_EVAL_STEPS, MAX_RECURSION_DEPTH};
pub use value::{
    DeviceId, FunctionId, MeasurementValue, PhysicalDimension, PhysicalValue, SpaceId,
    UnitDimension, Value,
};
pub use vm::VM;

use compact_str::CompactString;
use hwc_parser::ast::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Control flow signals during statement execution
#[derive(Debug, Clone, PartialEq)]
pub enum ControlFlow {
    Continue,
    Return(Value),
}

/// Evaluates a parsed Program entirely via the Bytecode VM
pub fn evaluate_program(program: &Program, ctx: &mut EvaluationContext) -> Result<(), EvalError> {
    if std::env::var_os("HWC_DEBUG").is_some() {
        eprintln!("[EVAL DEBUG] Starting Program Evaluation via Bytecode VM");
    }

    // Pass 1: Collect top-level declarations into context
    let mut script_stmts = Vec::new();
    let mut has_spaces = false;

    for item in &program.items {
        match item {
            TopLevelItem::Function(f) => {
                ctx.functions.insert(f.name.name.clone(), f.clone());
            }
            TopLevelItem::Struct(s) => {
                ctx.structs.insert(s.name.name.clone(), s.clone());
            }
            TopLevelItem::Statement(stmt) => {
                script_stmts.push(stmt.clone());
            }
            TopLevelItem::Space(_) => {
                has_spaces = true;
            }
            _ => {}
        }
    }

    // Pass 2: Compile all functions to Bytecode Chunks (including imported functions)
    let mut compiled_functions: FxHashMap<CompactString, Arc<Chunk>> = FxHashMap::default();
    for (name, func_decl) in &ctx.functions {
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[EVAL DEBUG] Compiling function: {}", name);
        }
        let chunk = BytecodeCompiler::compile_function(
            func_decl,
            ctx.unit_registry.as_deref(),
            &ctx.functions,
            &ctx.structs,
            &ctx.enum_types,
        )?;
        compiled_functions.insert(name.clone(), Arc::new(chunk));
    }

    // Pass 3: If top-level script statements exist, execute $script
    if !script_stmts.is_empty() {
        let script_chunk = BytecodeCompiler::compile_statements(
            "$script",
            &script_stmts,
            program.span,
            ctx.unit_registry.as_deref(),
            &ctx.functions,
            &ctx.structs,
            &ctx.enum_types,
        )?;

        let mut vm = VM::new(&mut *ctx.emitter);
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions.clone());

        vm.run_chunk(Arc::new(script_chunk), None)?;
    }

    // Pass 4: Evaluate space declarations sequentially on the Bytecode VM
    if has_spaces {
        let mut space_counter = 1;
        for item in &program.items {
            if let TopLevelItem::Space(space_decl) = item {
                if std::env::var_os("HWC_DEBUG").is_some() {
                    eprintln!("[EVAL DEBUG] Compiling and Executing Space: {} (id: {})", space_decl.name.name, space_counter);
                }

                // Allocate nets for space
                let mut allocated_nets = FxHashMap::default();
                for net_decl in &space_decl.nets {
                    let mut props = FxHashMap::default();
                    for (prop_name, prop_expr) in &net_decl.properties {
                        let val = eval_expression_bytecode(prop_expr, ctx.unit_registry.as_deref())?;
                        props.insert(prop_name.clone(), val);
                    }
                    let net_id = ctx.emitter.allocate_net(
                        space_counter,
                        net_decl.name.as_str(),
                        props,
                    )?;
                    allocated_nets.insert(net_decl.name.clone(), net_id);
                }

                let space_chunk = BytecodeCompiler::compile_space(
                    space_decl,
                    space_counter,
                    &allocated_nets,
                    ctx.unit_registry.as_deref(),
                    &ctx.functions,
                    &ctx.structs,
                    &ctx.enum_types,
                )?;

                let mut vm = VM::new(&mut *ctx.emitter);
                vm.unit_registry = ctx.unit_registry.clone();
                vm.register_functions(compiled_functions.clone());

                vm.run_chunk(Arc::new(space_chunk), Some(space_counter))?;
                space_counter += 1;
            }
        }
    } else if script_stmts.is_empty() {
        // If no spaces and no top-level statements, run main() if present or single function
        if let Some(main_chunk) = compiled_functions.get("main").cloned() {
            let mut vm = VM::new(&mut *ctx.emitter);
            vm.unit_registry = ctx.unit_registry.clone();
            vm.register_functions(compiled_functions.clone());
            vm.run_chunk(main_chunk, None)?;
        } else if compiled_functions.len() == 1 {
            let (_, chunk) = compiled_functions.iter().next().unwrap();
            let chunk = chunk.clone();
            let mut vm = VM::new(&mut *ctx.emitter);
            vm.unit_registry = ctx.unit_registry.clone();
            vm.register_functions(compiled_functions.clone());
            vm.run_chunk(chunk, None)?;
        }
    }

    if std::env::var_os("HWC_DEBUG").is_some() {
        eprintln!("[EVAL DEBUG] Program Evaluation Completed Successfully via Bytecode VM");
    }
    Ok(())
}

/// Executes a HardwareScript program as a compute script (pure runtime, zero space/meshing overhead).
pub fn run_script(
    program: &Program,
    ctx: &mut EvaluationContext,
    target_fn: Option<&str>,
) -> Result<Option<Value>, EvalError> {
    // Pass 1: Collect declarations and top-level statements
    let mut script_stmts = Vec::new();
    for item in &program.items {
        match item {
            TopLevelItem::Function(f) => {
                ctx.functions.insert(f.name.name.clone(), f.clone());
            }
            TopLevelItem::Struct(s) => {
                ctx.structs.insert(s.name.name.clone(), s.clone());
            }
            TopLevelItem::Statement(stmt) => {
                script_stmts.push(stmt.clone());
            }
            _ => {}
        }
    }

    // Pass 2: Compile all functions
    let mut compiled_functions: FxHashMap<CompactString, Arc<Chunk>> = FxHashMap::default();
    for (name, func_decl) in &ctx.functions {
        let chunk = BytecodeCompiler::compile_function(
            func_decl,
            ctx.unit_registry.as_deref(),
            &ctx.functions,
            &ctx.structs,
            &ctx.enum_types,
        )?;
        compiled_functions.insert(name.clone(), Arc::new(chunk));
    }

    // 1. If explicit top-level script statements exist, execute $script
    if !script_stmts.is_empty() {
        let script_chunk = BytecodeCompiler::compile_statements(
            "$script",
            &script_stmts,
            program.span,
            ctx.unit_registry.as_deref(),
            &ctx.functions,
            &ctx.structs,
            &ctx.enum_types,
        )?;

        let mut vm = VM::new(&mut *ctx.emitter);
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions);

        let val = vm.run_chunk(Arc::new(script_chunk), None)?;
        return Ok(Some(val));
    }

    // 2. If a specific function was requested, execute it
    if let Some(target) = target_fn {
        let chunk = compiled_functions.get(target).ok_or_else(|| EvalError::General {
            message: format!("Function '{}' not found in file", target),
        })?.clone();

        let mut vm = VM::new(&mut *ctx.emitter);
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions);

        let val = vm.run_chunk(chunk, None)?;
        return Ok(Some(val));
    }

    // 3. If fn main() exists, execute main()
    if let Some(main_chunk) = compiled_functions.get("main").cloned() {
        let mut vm = VM::new(&mut *ctx.emitter);
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions);

        let val = vm.run_chunk(main_chunk, None)?;
        return Ok(Some(val));
    }

    // 4. If exactly one function exists, execute it
    if compiled_functions.len() == 1 {
        let (_, chunk) = compiled_functions.iter().next().unwrap();
        let chunk = chunk.clone();
        let mut vm = VM::new(&mut *ctx.emitter);
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions);

        let val = vm.run_chunk(chunk, None)?;
        return Ok(Some(val));
    }

    if !compiled_functions.is_empty() {
        let fn_names: Vec<_> = compiled_functions.keys().map(|k| k.as_str()).collect();
        return Err(EvalError::General {
            message: format!(
                "File contains functions ({}) but no top-level script or main() function. Use --fn <name> to execute a specific function.",
                fn_names.join(", ")
            ),
        });
    }

    Err(EvalError::General {
        message: "File contains definitions but no executable script or main() function.".into(),
    })
}

/// Helper to evaluate a standalone expression string (with built-in math and standard units)
pub fn eval_expression_str(
    expr_str: &str,
    unit_registry: Option<&hwc_types::UnitRegistry>,
) -> Result<Value, EvalError> {
    let lexer = hwc_parser::Lexer::new(expr_str);
    let tokens = lexer.tokenize().map_err(|e| EvalError::General {
        message: format!("Lexical error in expression: {:?}", e),
    })?;

    let mut parser = hwc_parser::Parser::new(tokens);
    let expr = parser.parse_expression().map_err(|e| EvalError::General {
        message: format!("Syntax error in expression: {:?}", e),
    })?;

    eval_expression_bytecode(&expr, unit_registry)
}

/// Helper to evaluate a standalone expression via a temporary bytecode chunk
pub fn eval_expression_bytecode(
    expr: &Expression,
    unit_registry: Option<&hwc_types::UnitRegistry>,
) -> Result<Value, EvalError> {
    let mut compiler = BytecodeCompiler::new("expr", unit_registry);
    let r = compiler.compile_expression(expr)?;
    compiler.chunk.emit(OpCode::Return { val: r }, expr.span());
    let chunk = compiler.finish();
    let mut emitter = MemoryEmitter::new();
    let mut vm = VM::new(&mut emitter);
    vm.unit_registry = unit_registry.map(|u| Arc::new(u.clone()));
    vm.run_chunk(Arc::new(chunk), None)
}

/// Legacy evaluator wrapper for backwards compatibility
pub struct Evaluator<'a> {
    pub ctx: &'a mut EvaluationContext,
}

impl<'a> Evaluator<'a> {
    pub fn new(ctx: &'a mut EvaluationContext) -> Self {
        Self { ctx }
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<(), EvalError> {
        evaluate_program(program, self.ctx)
    }

    pub fn eval_expression(&mut self, expr: &Expression) -> Result<Value, EvalError> {
        eval_expression_bytecode(expr, self.ctx.unit_registry.as_deref())
    }
}

