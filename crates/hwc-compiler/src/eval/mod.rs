//! HardwareScript v0.3.1 Comptime Evaluation Engine (`hwc-eval`) (Phase 2)
//!
//! Powered entirely by a Linear Bytecode Virtual Machine with static activation records,
//! deterministic fuel budgeting, host RAM protection, and pure Salsa geometry buffering.

pub mod builtins;
pub mod compiler;
pub mod context;
pub mod emitter;
pub mod escape_contract;
pub mod frame;
pub mod geometry_record;
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
pub use geometry_record::{
    CompactGeometryRecordHeader, FlatGeometryBuffer, GeometryBuffer, GeometryRecord,
};
pub use opcodes::{Chunk, ConstantIndex, JumpOffset, OpCode, Register};
pub use sandbox::{
    calculate_fuel, DeterministicGuard, SandboxError, DEFAULT_BASE_FUEL, DEFAULT_MAX_MEMORY_BYTES,
    FUEL_PER_MM2, MAX_CALL_STACK_DEPTH,
};
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

/// Evaluates a parsed Program entirely via the Bytecode VM with deterministic fuel scaling.
pub fn evaluate_program(program: &Program, ctx: &mut EvaluationContext) -> Result<(), EvalError> {
    if std::env::var_os("HWC_DEBUG").is_some() {
            }

    // Pass 1: Collect top-level declarations into context
    let mut script_stmts = Vec::new();
    let mut has_spaces = false;

    for item in &program.items {
        match item {
            TopLevelItem::Const(c) => {
                let val = eval_expression_bytecode_with_context(
                    &c.value,
                    ctx.unit_registry.as_deref(),
                    &ctx.functions,
                    &ctx.structs,
                    &ctx.enum_types,
                    &ctx.constants,
                )?;
                ctx.insert_variable(c.name.name.clone(), val);
            }
            TopLevelItem::Function(f) => {
                ctx.functions.insert(f.name.name.clone(), f.clone());
            }
            TopLevelItem::Struct(s) => {
                ctx.structs.insert(s.name.name.clone(), s.clone());
            }
            TopLevelItem::Impl(imp) => {
                for method in &imp.methods {
                    let qualified: CompactString = format!("{}::{}", imp.target.name, method.name.name).into();
                    ctx.functions.insert(qualified, method.clone());
                }
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

    // Pass 2: Compile all functions to Bytecode Chunks (including imported functions and impl methods)
    let mut compiled_functions: FxHashMap<CompactString, Arc<Chunk>> = FxHashMap::default();
    for (name, func_decl) in &ctx.functions {
        let chunk = BytecodeCompiler::compile_function(
            func_decl,
            ctx.unit_registry.as_deref(),
            &ctx.functions,
            &ctx.structs,
            &ctx.enum_types,
            &ctx.constants,
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
            &ctx.constants,
        )?;

        let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions.clone());

        vm.run_chunk(Arc::new(script_chunk), None)?;
    }

    // Pass 4: Evaluate space declarations sequentially on the Bytecode VM
    if has_spaces {
        let mut space_counter = 1;
        for item in &program.items {
            if let TopLevelItem::Space(space_decl) = item {
                // Allocate nets for space
                let mut allocated_nets = FxHashMap::default();
                for net_decl in &space_decl.nets {
                    let mut props = FxHashMap::default();
                    for (prop_name, prop_expr) in &net_decl.properties {
                        let val = match prop_expr {
                            Expression::Variable { name, .. } => Value::String(name.as_str().into()),
                            _ => eval_expression_bytecode_with_context(
                                prop_expr,
                                ctx.unit_registry.as_deref(),
                                &ctx.functions,
                                &ctx.structs,
                                &ctx.enum_types,
                                &ctx.constants,
                            )?,
                        };
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
                    &ctx.constants,
                )?;

                let mut w_pm = None;
                let mut h_pm = None;
                if let Some((w_expr, h_expr)) = &space_decl.dimensions {
                    if let Ok(w_val) = eval_expression_bytecode_with_context(
                        w_expr,
                        ctx.unit_registry.as_deref(),
                        &ctx.functions,
                        &ctx.structs,
                        &ctx.enum_types,
                        &ctx.constants,
                    ) {
                        if let Value::Measurement(m) = w_val {
                            w_pm = Some(m.raw);
                        }
                    }
                    if let Ok(h_val) = eval_expression_bytecode_with_context(
                        h_expr,
                        ctx.unit_registry.as_deref(),
                        &ctx.functions,
                        &ctx.structs,
                        &ctx.enum_types,
                        &ctx.constants,
                    ) {
                        if let Value::Measurement(m) = h_val {
                            h_pm = Some(m.raw);
                        }
                    }
                }

                let fuel = calculate_fuel(w_pm, h_pm, space_decl.comptime_fuel());
                let guard = DeterministicGuard::with_fuel(fuel);

                let mut vm = VM::with_guard(&mut *ctx.emitter, guard);
                vm.unit_registry = ctx.unit_registry.clone();
                vm.register_functions(compiled_functions.clone());

                vm.run_chunk(Arc::new(space_chunk), Some(space_counter))?;
                space_counter += 1;
            }
        }
    } else if script_stmts.is_empty() {
        // If no spaces and no top-level statements, run main() if present or single function
        if let Some(main_chunk) = compiled_functions.get("main").cloned() {
            let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
            vm.unit_registry = ctx.unit_registry.clone();
            vm.register_functions(compiled_functions.clone());
            vm.run_chunk(main_chunk, None)?;
        } else if compiled_functions.len() == 1 {
            if let Some((_, chunk)) = compiled_functions.iter().next() {
                let chunk = chunk.clone();
                let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
                vm.unit_registry = ctx.unit_registry.clone();
                vm.register_functions(compiled_functions.clone());
                vm.run_chunk(chunk, None)?;
            }
        }
    }

    if std::env::var_os("HWC_DEBUG").is_some() {
            }
    Ok(())
}

/// Evaluates a single space declaration directly into a pure GeometryBuffer (Salsa query pure execution).
pub fn evaluate_space_to_buffer(
    space_decl: &SpaceDecl,
    compiled_functions: &FxHashMap<CompactString, Arc<Chunk>>,
    unit_registry: Option<&hwc_types::UnitRegistry>,
    structs: &FxHashMap<CompactString, StructDecl>,
    enum_types: &FxHashMap<CompactString, Value>,
    functions: &FxHashMap<CompactString, FunctionDecl>,
    constants: &FxHashMap<CompactString, Value>,
) -> Result<GeometryBuffer, EvalError> {
    let mut memory_emitter = MemoryEmitter::new();
    let mut allocated_nets = FxHashMap::default();
    let space_id = 1;

    for net_decl in &space_decl.nets {
        let mut props = FxHashMap::default();
        for (prop_name, prop_expr) in &net_decl.properties {
            let val = match prop_expr {
                Expression::Variable { name, .. } => Value::String(name.as_str().into()),
                _ => eval_expression_bytecode_with_context(
                    prop_expr,
                    unit_registry,
                    functions,
                    structs,
                    enum_types,
                    constants,
                )?,
            };
            props.insert(prop_name.clone(), val);
        }
        let net_id = memory_emitter.allocate_net(space_id, net_decl.name.as_str(), props)?;
        allocated_nets.insert(net_decl.name.clone(), net_id);
    }

    let space_chunk = BytecodeCompiler::compile_space(
        space_decl,
        space_id,
        &allocated_nets,
        unit_registry,
        functions,
        structs,
        enum_types,
        constants,
    )?;

    let mut w_pm = None;
    let mut h_pm = None;
    if let Some((w_expr, h_expr)) = &space_decl.dimensions {
        if let Ok(w_val) = eval_expression_bytecode_with_context(
            w_expr,
            unit_registry,
            functions,
            structs,
            enum_types,
            constants,
        ) {
            if let Value::Measurement(m) = w_val {
                w_pm = Some(m.raw);
            }
        }
        if let Ok(h_val) = eval_expression_bytecode_with_context(
            h_expr,
            unit_registry,
            functions,
            structs,
            enum_types,
            constants,
        ) {
            if let Value::Measurement(m) = h_val {
                h_pm = Some(m.raw);
            }
        }
    }

    let fuel = calculate_fuel(w_pm, h_pm, space_decl.comptime_fuel());
    let guard = DeterministicGuard::with_fuel(fuel);
    let mut output_buffer = GeometryBuffer::new();

    let mut vm = VM::with_output_buffer(&mut memory_emitter, &mut output_buffer, guard);
    vm.unit_registry = unit_registry.map(|u| Arc::new(u.clone()));
    vm.register_functions(compiled_functions.clone());

    vm.run_chunk(Arc::new(space_chunk), Some(space_id))?;

    Ok(output_buffer)
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
            TopLevelItem::Const(c) => {
                let val = eval_expression_bytecode_with_context(
                    &c.value,
                    ctx.unit_registry.as_deref(),
                    &ctx.functions,
                    &ctx.structs,
                    &ctx.enum_types,
                    &ctx.constants,
                )?;
                ctx.insert_variable(c.name.name.clone(), val);
            }
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
            &ctx.constants,
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
            &ctx.constants,
        )?;

        let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions.clone());

        let val = vm.run_chunk(Arc::new(script_chunk), None)?;
        return Ok(Some(val));
    }

    // 2. If a specific function was requested, execute it
    if let Some(target) = target_fn {
        let chunk = compiled_functions.get(target).ok_or_else(|| EvalError::General {
            message: format!("Function '{}' not found in file", target),
        })?.clone();

        let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions.clone());

        let val = vm.run_chunk(chunk, None)?;
        return Ok(Some(val));
    }

    // 3. If fn main() exists, execute main()
    if let Some(main_chunk) = compiled_functions.get("main").cloned() {
        let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
        vm.unit_registry = ctx.unit_registry.clone();
        vm.register_functions(compiled_functions.clone());

        let val = vm.run_chunk(main_chunk, None)?;
        return Ok(Some(val));
    }

    // 4. If exactly one function exists, execute it
    if compiled_functions.len() == 1 {
        if let Some((_, chunk)) = compiled_functions.iter().next() {
            let chunk = chunk.clone();
            let mut vm = VM::with_guard(&mut *ctx.emitter, ctx.sandbox.clone());
            vm.unit_registry = ctx.unit_registry.clone();
            vm.register_functions(compiled_functions.clone());

            let val = vm.run_chunk(chunk, None)?;
            return Ok(Some(val));
        }
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
    eval_expression_bytecode_with_context(
        expr,
        unit_registry,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &FxHashMap::default(),
        &FxHashMap::default(),
    )
}

/// Helper to evaluate a standalone expression with known context via a temporary bytecode chunk
pub fn eval_expression_bytecode_with_context(
    expr: &Expression,
    unit_registry: Option<&hwc_types::UnitRegistry>,
    functions: &FxHashMap<CompactString, FunctionDecl>,
    structs: &FxHashMap<CompactString, StructDecl>,
    enum_types: &FxHashMap<CompactString, Value>,
    constants: &FxHashMap<CompactString, Value>,
) -> Result<Value, EvalError> {
    let mut compiler = BytecodeCompiler::new("expr", unit_registry);
    compiler.function_decls = functions.clone();
    compiler.struct_decls = structs.clone();
    compiler.enum_types = enum_types.clone();
    compiler.constants = constants.clone();
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
        eval_expression_bytecode_with_context(
            expr,
            self.ctx.unit_registry.as_deref(),
            &self.ctx.functions,
            &self.ctx.structs,
            &self.ctx.enum_types,
            &self.ctx.constants,
        )
    }
}
