use compact_str::CompactString;
use hwc_types::UnitRegistry;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::super::context::EvalError;
use super::super::emitter::SpaceEmitter;
use super::super::frame::CallFrame;
use super::super::geometry_record::GeometryBuffer;
use super::super::opcodes::Chunk;
use super::super::sandbox::DeterministicGuard;
use super::super::value::Value;
use hwc_engine::entity_graph::identity::HierarchicalPath;

/// Bytecode Virtual Machine
pub struct VM<'a> {
    pub stack: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub functions: FxHashMap<CompactString, Arc<Chunk>>,
    pub guard: DeterministicGuard,
    pub current_space_id: Option<u32>,
    pub emitter: &'a mut dyn SpaceEmitter,
    pub output_buffer: Option<&'a mut GeometryBuffer>,
    pub unit_registry: Option<Arc<UnitRegistry>>,
    pub emitted_record_count: u32,
}

impl<'a> VM<'a> {
    pub fn new(emitter: &'a mut dyn SpaceEmitter) -> Self {
        Self {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(256),
            functions: FxHashMap::default(),
            guard: DeterministicGuard::default(),
            current_space_id: None,
            emitter,
            output_buffer: None,
            unit_registry: None,
            emitted_record_count: 0,
        }
    }

    pub fn with_guard(emitter: &'a mut dyn SpaceEmitter, guard: DeterministicGuard) -> Self {
        Self {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(256),
            functions: FxHashMap::default(),
            guard,
            current_space_id: None,
            emitter,
            output_buffer: None,
            unit_registry: None,
            emitted_record_count: 0,
        }
    }

    pub fn with_output_buffer(
        emitter: &'a mut dyn SpaceEmitter,
        output_buffer: &'a mut GeometryBuffer,
        guard: DeterministicGuard,
    ) -> Self {
        Self {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(256),
            functions: FxHashMap::default(),
            guard,
            current_space_id: None,
            emitter,
            output_buffer: Some(output_buffer),
            unit_registry: None,
            emitted_record_count: 0,
        }
    }

    pub fn register_function(&mut self, name: impl Into<CompactString>, chunk: Arc<Chunk>) {
        self.functions.insert(name.into(), chunk);
    }

    pub fn register_functions(&mut self, funcs: FxHashMap<CompactString, Arc<Chunk>>) {
        self.functions.extend(funcs);
    }

    /// Execute a chunk to completion
    pub fn run_chunk(&mut self, chunk: Arc<Chunk>, space_id: Option<u32>) -> Result<Value, EvalError> {
        self.current_space_id = space_id;

        let stack_base = self.stack.len();
        let num_regs = (chunk.max_registers as usize).max(64);
        self.stack.resize(stack_base + num_regs, Value::Void);

        let root_path = if let Some(sid) = space_id {
            HierarchicalPath::root(&format!("Space_{}", sid))
        } else {
            HierarchicalPath::root(chunk.name.as_str())
        };

        self.frames.push(CallFrame::with_path(
            chunk,
            stack_base,
            None,
            "main",
            root_path,
        ));

        self.run()
    }

    pub(crate) fn eval_cmp<F>(&self, left: &Value, right: &Value, cmp: F) -> Result<Value, EvalError>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(cmp(*a as f64, *b as f64))),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(cmp(*a, *b))),
            (Value::Measurement(a), Value::Measurement(b)) => {
                if a.dimension != b.dimension {
                    return Err(EvalError::UnitMismatch {
                        expected: a.dimension,
                        found: b.dimension,
                        op: "comparison",
                    });
                }
                Ok(Value::Bool(cmp(a.raw as f64, b.raw as f64)))
            }
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Comparable numeric types",
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }
}
