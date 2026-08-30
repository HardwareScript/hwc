//! HardwareScript v0.3.0 AST-to-Bytecode Compiler
//!
//! Compiles immutable parsed AST (`Program`, `SpaceDecl`, `FunctionDecl`, `Statement`, `Expression`)
//! into optimized bytecode `Chunk` streams for execution on the VM.

use compact_str::CompactString;
use hwc_parser::ast::*;
use hwc_types::{NetId, UnitRegistry};
use rustc_hash::FxHashMap;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{Chunk, OpCode, Register};
use crate::eval::value::{SpaceId, Value};

mod scope;
mod expression;
mod statement;
mod space_methods;

pub use scope::Scope;

/// Loop compilation context for `break` and `continue` jumps
#[derive(Debug, Clone)]
pub struct LoopContext {
    pub loop_start_ip: usize,
    pub step_ip: Option<usize>,
    pub break_jumps: Vec<usize>,
    pub continue_jumps: Vec<usize>,
}

/// AST to Bytecode Compiler
pub struct BytecodeCompiler<'a> {
    pub chunk: Chunk,
    pub scopes: Vec<Scope>,
    pub loop_stack: Vec<LoopContext>,
    pub next_reg: u16,
    pub max_reg: u16,
    pub unit_registry: Option<&'a UnitRegistry>,
    pub function_decls: FxHashMap<CompactString, FunctionDecl>,
    pub struct_decls: FxHashMap<CompactString, StructDecl>,
    pub enum_types: FxHashMap<CompactString, Value>,
}

impl<'a> BytecodeCompiler<'a> {
    pub fn new(name: impl Into<CompactString>, unit_registry: Option<&'a UnitRegistry>) -> Self {
        Self {
            chunk: Chunk::new(name),
            scopes: vec![Scope::default()],
            loop_stack: Vec::new(),
            next_reg: 0,
            max_reg: 0,
            unit_registry,
            function_decls: FxHashMap::default(),
            struct_decls: FxHashMap::default(),
            enum_types: FxHashMap::default(),
        }
    }

    pub fn alloc_reg(&mut self) -> Register {
        let r = Register(self.next_reg);
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        r
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn bind_var(&mut self, name: impl Into<CompactString>, reg: Register, is_mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bind(name, reg, is_mutable);
        }
    }

    pub fn lookup_var(&self, name: &str) -> Option<(Register, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.lookup(name) {
                return Some(entry);
            }
        }
        None
    }

    pub fn finish(mut self) -> Chunk {
        self.chunk.max_registers = self.max_reg;
        self.chunk
    }

    /// Compile a sequence of statements (e.g. top-level script statements) into a self-contained `Chunk`
    pub fn compile_statements(
        name: impl Into<CompactString>,
        statements: &[Statement],
        span: Span,
        unit_registry: Option<&'a UnitRegistry>,
        all_funcs: &FxHashMap<CompactString, FunctionDecl>,
        all_structs: &FxHashMap<CompactString, StructDecl>,
        enum_types: &FxHashMap<CompactString, Value>,
    ) -> Result<Chunk, EvalError> {
        let name_str: CompactString = name.into();
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[BYTECODE DEBUG] Compiling statements chunk '{}'", name_str);
        }
        let mut compiler = BytecodeCompiler::new(name_str.clone(), unit_registry);
        compiler.function_decls = all_funcs.clone();
        compiler.struct_decls = all_structs.clone();
        compiler.enum_types = enum_types.clone();

        let mut has_explicit_return = false;
        for stmt in statements {
            if let Statement::Return { .. } = stmt {
                has_explicit_return = true;
            }
            compiler.compile_statement(stmt)?;
        }

        if !has_explicit_return {
            let void_reg = compiler.alloc_reg();
            compiler.chunk.emit(
                OpCode::LoadNull { dst: void_reg },
                span,
            );
            compiler.chunk.emit(
                OpCode::Return { val: void_reg },
                span,
            );
        }

        let chunk = compiler.finish();
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[BYTECODE DEBUG] Finished statements chunk '{}':\n{}", name_str, chunk.disassemble());
        }
        Ok(chunk)
    }

    /// Compile a function declaration into a self-contained `Chunk`
    pub fn compile_function(
        decl: &FunctionDecl,
        unit_registry: Option<&'a UnitRegistry>,
        all_funcs: &FxHashMap<CompactString, FunctionDecl>,
        all_structs: &FxHashMap<CompactString, StructDecl>,
        enum_types: &FxHashMap<CompactString, Value>,
    ) -> Result<Chunk, EvalError> {
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[BYTECODE DEBUG] Compiling function '{}'", decl.name.name);
        }
        let mut compiler = BytecodeCompiler::new(decl.name.name.clone(), unit_registry);
        compiler.function_decls = all_funcs.clone();
        compiler.struct_decls = all_structs.clone();
        compiler.enum_types = enum_types.clone();

        // ── PASS 1: Parameters MUST occupy registers R0..R(N-1) strictly ─────
        let mut param_bindings = Vec::new();
        for param in &decl.parameters {
            let r = compiler.alloc_reg();
            compiler.bind_var(param.name.clone(), r, false);
            param_bindings.push((param, r));
        }

        // ── PASS 2: Coerce types (e.g. Point2D) into fresh registers (>= N) ───
        for (param, r) in param_bindings {
            if let TypeExpr::Named { name: type_name, .. } = &param.type_annotation {
                if type_name.as_str() == "Point2D" {
                    let coerced_r = compiler.alloc_reg();
                    compiler.chunk.emit(
                        OpCode::CoercePoint2D {
                            dst: coerced_r,
                            src: r,
                        },
                        param.span,
                    );
                    // Rebind parameter variable to the coerced register
                    compiler.bind_var(param.name.clone(), coerced_r, false);
                }
            }
        }

        // Compile body statements
        let mut has_explicit_return = false;
        for stmt in &decl.body.statements {
            if let Statement::Return { .. } = stmt {
                has_explicit_return = true;
            }
            compiler.compile_statement(stmt)?;
        }

        // Evaluate tail expression if present
        if let Some(tail) = &decl.body.tail_expr {
            let ret_reg = compiler.compile_expression(tail)?;
            compiler.chunk.emit(
                OpCode::Return { val: ret_reg },
                decl.span,
            );
        } else if !has_explicit_return {
            let void_reg = compiler.alloc_reg();
            compiler.chunk.emit(
                OpCode::LoadNull { dst: void_reg },
                decl.span,
            );
            compiler.chunk.emit(
                OpCode::Return { val: void_reg },
                decl.span,
            );
        }

        let chunk = compiler.finish();
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[BYTECODE DEBUG] Finished function '{}':\n{}", decl.name.name, chunk.disassemble());
        }
        Ok(chunk)
    }

    /// Compile a `space` block into a runnable chunk
    pub fn compile_space(
        space: &SpaceDecl,
        space_id: u32,
        allocated_nets: &FxHashMap<CompactString, NetId>,
        unit_registry: Option<&'a UnitRegistry>,
        all_funcs: &FxHashMap<CompactString, FunctionDecl>,
        all_structs: &FxHashMap<CompactString, StructDecl>,
        enum_types: &FxHashMap<CompactString, Value>,
    ) -> Result<Chunk, EvalError> {
        if std::env::var_os("HWC_DEBUG").is_some() {
            eprintln!("[BYTECODE DEBUG] Compiling space '{}' (id: {})", space.name.name, space_id);
        }

        let mut compiler = BytecodeCompiler::new(space.name.name.clone(), unit_registry);
        compiler.function_decls = all_funcs.clone();
        compiler.struct_decls = all_structs.clone();
        compiler.enum_types = enum_types.clone();

        // 1. Implicit space handle binding
        let space_handle_reg = compiler.alloc_reg();
        let space_const = compiler.chunk.add_constant(Value::SpaceHandle(SpaceId(space_id)));
        compiler.chunk.emit(
            OpCode::LoadConst {
                dst: space_handle_reg,
                const_idx: space_const,
            },
            space.span,
        );
        compiler.bind_var("space", space_handle_reg, false);

        // 2. Nets block: bind each NetId as a NetHandle constant in a register
        for net_decl in &space.nets {
            let net_reg = compiler.alloc_reg();
            let net_id = allocated_nets.get(&net_decl.name).copied().unwrap_or(NetId::UNCONNECTED);
            let net_const = compiler.chunk.add_constant(Value::NetHandle(net_id));
            compiler.chunk.emit(
                OpCode::LoadConst {
                    dst: net_reg,
                    const_idx: net_const,
                },
                net_decl.span,
            );
            compiler.bind_var(net_decl.name.clone(), net_reg, false);
        }

        // 3. Space statements
        for stmt in &space.statements {
            compiler.compile_statement(stmt)?;
        }

        // Return void
        let void_r = compiler.alloc_reg();
        compiler.chunk.emit(OpCode::LoadNull { dst: void_r }, space.span);
        compiler.chunk.emit(OpCode::Return { val: void_r }, space.span);

        let chunk = compiler.finish();
        eprintln!("[BYTECODE DEBUG] Finished space '{}':\n{}", space.name.name, chunk.disassemble());
        Ok(chunk)
    }
}
