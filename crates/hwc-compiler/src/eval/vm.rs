//! HardwareScript v0.3.0 Bytecode Virtual Machine (`hwc-eval`)
//!
//! Executes linear bytecode chunks on a flat activation stack with static activation records.

use compact_str::CompactString;
use hwc_engine::entity_graph::identity::{EntityId, HierarchicalPath, PathSegment};
use hwc_types::UnitRegistry;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::builtins;
use super::context::EvalError;
use super::emitter::SpaceEmitter;
use super::frame::CallFrame;
use super::geometry_record::{GeometryBuffer, GeometryRecord};
use super::opcodes::{Chunk, OpCode};
use super::sandbox::{DeterministicGuard, SandboxError, MAX_CALL_STACK_DEPTH};
use super::value::{MeasurementValue, SpaceId, Value};

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

    /// Main instruction dispatch loop
    pub fn run(&mut self) -> Result<Value, EvalError> {
        while !self.frames.is_empty() {
            self.guard.consume_step()?;

            let frame_idx = self.frames.len() - 1;

            let (op, base) = {
                let frame = &mut self.frames[frame_idx];
                if frame.ip >= frame.chunk.code.len() {
                    let popped = self.frames.pop().ok_or(EvalError::General { message: "Call stack underflow".into() })?;
                    self.stack.truncate(popped.stack_base);
                    if self.frames.is_empty() {
                        return Ok(Value::Void);
                    }
                    continue;
                }
                let op = frame.chunk.code[frame.ip].clone();
                frame.ip += 1;
                (op, frame.stack_base)
            };

            match op {
                OpCode::LoadConst { dst, const_idx } => {
                    let val = self.frames[frame_idx].chunk.constants[const_idx.0 as usize].clone();
                    self.stack[base + dst.0 as usize] = val;
                }

                OpCode::Move { dst, src } => {
                    self.stack[base + dst.0 as usize] = self.stack[base + src.0 as usize].clone();
                }

                OpCode::LoadNull { dst } => {
                    self.stack[base + dst.0 as usize] = Value::Void;
                }

                OpCode::LoadBool { dst, val } => {
                    self.stack[base + dst.0 as usize] = Value::Bool(val);
                }

                OpCode::LoadInt { dst, val } => {
                    self.stack[base + dst.0 as usize] = Value::Int(val);
                }

                OpCode::LoadFloat { dst, val } => {
                    self.stack[base + dst.0 as usize] = Value::Float(val);
                }

                OpCode::Add { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.add(r)?;
                }

                OpCode::Sub { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.sub(r)?;
                }

                OpCode::Mul { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.mul(r)?;
                }

                OpCode::Div { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.div(r)?;
                }

                OpCode::Mod { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.modulo(r)?;
                }

                OpCode::Neg { dst, src } => {
                    let val = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = val.neg()?;
                }

                OpCode::Eq { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = Value::Bool(l == r);
                }

                OpCode::Ne { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = Value::Bool(l != r);
                }

                OpCode::Lt { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = self.eval_cmp(l, r, |a, b| a < b)?;
                }

                OpCode::Le { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = self.eval_cmp(l, r, |a, b| a <= b)?;
                }

                OpCode::Gt { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = self.eval_cmp(l, r, |a, b| a > b)?;
                }

                OpCode::Ge { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = self.eval_cmp(l, r, |a, b| a >= b)?;
                }

                OpCode::Not { dst, src } => {
                    let val = &self.stack[base + src.0 as usize];
                    let res = match val {
                        Value::Bool(b) => Value::Bool(!b),
                        Value::Int(i) => Value::Bool(*i == 0),
                        other => return Err(EvalError::TypeMismatch { expected: "Bool", found: other.type_name().to_string() }),
                    };
                    self.stack[base + dst.0 as usize] = res;
                }

                OpCode::And { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    let a = match l { Value::Bool(b) => *b, Value::Int(i) => *i != 0, _ => false };
                    let b = match r { Value::Bool(b) => *b, Value::Int(i) => *i != 0, _ => false };
                    self.stack[base + dst.0 as usize] = Value::Bool(a && b);
                }

                OpCode::Or { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    let a = match l { Value::Bool(b) => *b, Value::Int(i) => *i != 0, _ => false };
                    let b = match r { Value::Bool(b) => *b, Value::Int(i) => *i != 0, _ => false };
                    self.stack[base + dst.0 as usize] = Value::Bool(a || b);
                }

                // ── Bitwise & Shift ──
                OpCode::BitwiseAnd { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.bitwise_and(r)?;
                }

                OpCode::BitwiseOr { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.bitwise_or(r)?;
                }

                OpCode::BitwiseXor { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.bitwise_xor(r)?;
                }

                OpCode::BitwiseNot { dst, src } => {
                    let s = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = s.bitwise_not()?;
                }

                OpCode::ShiftLeft { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.shift_left(r)?;
                }

                OpCode::ShiftRight { dst, lhs, rhs } => {
                    let l = &self.stack[base + lhs.0 as usize];
                    let r = &self.stack[base + rhs.0 as usize];
                    self.stack[base + dst.0 as usize] = l.shift_right(r)?;
                }

                OpCode::Jump { offset } => {
                    let frame = &mut self.frames[frame_idx];
                    frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                }

                OpCode::JumpIfTrue { cond, offset } => {
                    let c = match &self.stack[base + cond.0 as usize] {
                        Value::Bool(b) => *b,
                        Value::Int(i) => *i != 0,
                        _ => false,
                    };
                    if c {
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                    }
                }

                OpCode::JumpIfFalse { cond, offset } => {
                    let c = match &self.stack[base + cond.0 as usize] {
                        Value::Bool(b) => *b,
                        Value::Int(i) => *i != 0,
                        _ => false,
                    };
                    if !c {
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                    }
                }

                OpCode::LoopStep { iter_reg, end_reg, step_val, offset } => {
                    let cur = match self.stack[base + iter_reg.0 as usize] {
                        Value::Int(i) => i,
                        _ => 0,
                    };
                    let end = match self.stack[base + end_reg.0 as usize] {
                        Value::Int(i) => i,
                        _ => 0,
                    };
                    let next = cur + step_val;
                    self.stack[base + iter_reg.0 as usize] = Value::Int(next);
                    if (step_val > 0 && next <= end) || (step_val < 0 && next >= end) {
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                    }
                }

                OpCode::Call { func_name_idx, args_start, arg_count, dst } => {
                    if self.frames.len() >= MAX_CALL_STACK_DEPTH {
                        return Err(SandboxError::RecursionDepthExceeded { max_depth: MAX_CALL_STACK_DEPTH }.into());
                    }
                    let func_name = self.frames[frame_idx].chunk.constants[func_name_idx.0 as usize].as_compact_str()?.clone();
                    let target_chunk = self.functions.get(&func_name).cloned().ok_or_else(|| {
                        EvalError::UnknownFunction { name: func_name.clone() }
                    })?;

                    let new_base = self.stack.len();
                    let num_regs = (target_chunk.max_registers as usize).max(64);
                    // Copy arguments into callee's initial registers
                    for i in 0..arg_count {
                        let arg_val = self.stack[base + args_start.0 as usize + i as usize].clone();
                        self.stack.push(arg_val);
                    }
                    // Resize to accommodate all registers in target chunk
                    self.stack.resize(new_base + num_regs, Value::Void);

                    let mut callee_path = self.frames[frame_idx].path.clone();
                    callee_path.push(PathSegment::SubCell(func_name.clone()));

                    self.frames.push(CallFrame::with_path(
                        target_chunk,
                        new_base,
                        Some(dst),
                        func_name,
                        callee_path,
                    ));
                }

                OpCode::Return { val } => {
                    let return_val = self.stack[base + val.0 as usize].clone();
                    let popped = self.frames.pop().ok_or(EvalError::General { message: "Call stack underflow on return".into() })?;
                    self.stack.truncate(popped.stack_base);

                    if let Some(parent_frame) = self.frames.last() {
                        if let Some(ret_reg) = popped.return_register {
                            self.stack[parent_frame.stack_base + ret_reg.0 as usize] = return_val;
                        }
                    } else {
                        return Ok(return_val);
                    }
                }

                OpCode::AllocArray { dst, start_reg, count } => {
                    let mut elements = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        elements.push(self.stack[base + start_reg.0 as usize + i as usize].clone());
                    }
                    let byte_size = (count as usize) * std::mem::size_of::<Value>();
                    self.guard.track_allocation(byte_size)?;
                    self.stack[base + dst.0 as usize] = Value::Array(Arc::new(elements));
                }

                OpCode::AllocStruct { dst, struct_name_idx, fields_start, count } => {
                    let struct_meta = self.frames[frame_idx].chunk.constants[struct_name_idx.0 as usize].clone();
                    if let Value::StructInstance { name, fields } = struct_meta {
                        let mut initialized_fields = Vec::with_capacity(count as usize);
                        for (i, (fname, _)) in fields.iter().enumerate() {
                            let val = if (i as u16) < count {
                                self.stack[base + fields_start.0 as usize + i].clone()
                            } else {
                                Value::Void
                            };
                            initialized_fields.push((fname.clone(), val));
                        }
                        self.stack[base + dst.0 as usize] = Value::StructInstance {
                            name,
                            fields: Arc::new(initialized_fields),
                        };
                    } else {
                        self.stack[base + dst.0 as usize] = Value::Void;
                    }
                }

                OpCode::GetField { dst, obj, field_idx } => {
                    let field_name = self.frames[frame_idx].chunk.constants[field_idx.0 as usize].as_compact_str()?.clone();
                    let target_obj = &self.stack[base + obj.0 as usize];
                                        match target_obj {
                        Value::StructInstance { fields, name } => {
                            let val = fields
                                .iter()
                                .find(|(k, _)| k == &field_name)
                                .map(|(_, v)| v.clone())
                                .ok_or_else(|| EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: name.clone(),
                                })?;
                                                        self.stack[base + dst.0 as usize] = val;
                        }
                        Value::EnumType { name, variants } => {
                            let val = variants.get(&field_name).cloned().ok_or_else(|| {
                                EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: name.clone(),
                                }
                            })?;
                                                        self.stack[base + dst.0 as usize] = val;
                        }
                        Value::Point2D { x, y } => {
                            let result = match field_name.as_str() {
                                "x" => Value::Measurement(MeasurementValue::length_pm(*x as i128)),
                                "y" => Value::Measurement(MeasurementValue::length_pm(*y as i128)),
                                _ => return Err(EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: CompactString::new("Point2D"),
                                }),
                            };
                                                        self.stack[base + dst.0 as usize] = result;
                        }
                        Value::Point3D { x, y, z } => {
                            let result = match field_name.as_str() {
                                "x" => Value::Measurement(MeasurementValue::length_pm(*x as i128)),
                                "y" => Value::Measurement(MeasurementValue::length_pm(*y as i128)),
                                "z" => Value::Measurement(MeasurementValue::length_pm(*z as i128)),
                                _ => return Err(EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: CompactString::new("Point3D"),
                                }),
                            };
                                                        self.stack[base + dst.0 as usize] = result;
                        }
                        Value::Vector2D { dx, dy } => {
                            let result = match field_name.as_str() {
                                "dx" => Value::Measurement(MeasurementValue::length_pm(*dx as i128)),
                                "dy" => Value::Measurement(MeasurementValue::length_pm(*dy as i128)),
                                _ => return Err(EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: CompactString::new("Vector2D"),
                                }),
                            };
                                                        self.stack[base + dst.0 as usize] = result;
                        }
                        Value::BoundingBox { min_x, min_y, max_x, max_y } => {
                            let result = match field_name.as_str() {
                                "min_x" => Value::Measurement(MeasurementValue::length_pm(*min_x as i128)),
                                "min_y" => Value::Measurement(MeasurementValue::length_pm(*min_y as i128)),
                                "max_x" => Value::Measurement(MeasurementValue::length_pm(*max_x as i128)),
                                "max_y" => Value::Measurement(MeasurementValue::length_pm(*max_y as i128)),
                                _ => return Err(EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: CompactString::new("BoundingBox"),
                                }),
                            };
                                                        self.stack[base + dst.0 as usize] = result;
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "StructInstance, EnumType, Point2D, Point3D, Vector2D, or BoundingBox",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::SetField { obj, field_idx, src } => {
                    let field_name = self.frames[frame_idx].chunk.constants[field_idx.0 as usize].as_compact_str()?.clone();
                    let src_val = self.stack[base + src.0 as usize].clone();
                    let target_obj = &mut self.stack[base + obj.0 as usize];
                    match target_obj {
                        Value::StructInstance { fields, .. } => {
                            let mut new_fields = (**fields).clone();
                            if let Some(entry) = new_fields.iter_mut().find(|(k, _)| k == &field_name) {
                                entry.1 = src_val;
                                *fields = Arc::new(new_fields);
                            }
                        }
                        _ => {}
                    }
                }

                OpCode::GetIndex { dst, obj, index } => {
                    let target_obj = &self.stack[base + obj.0 as usize];
                    let idx_val = &self.stack[base + index.0 as usize];
                    match (target_obj, idx_val) {
                        (Value::Array(items) | Value::Tuple(items), Value::Int(i)) => {
                            if *i >= 0 && (*i as usize) < items.len() {
                                self.stack[base + dst.0 as usize] = items[*i as usize].clone();
                            } else {
                                self.stack[base + dst.0 as usize] = Value::Void;
                            }
                        }
                        _ => {
                            self.stack[base + dst.0 as usize] = Value::Void;
                        }
                    }
                }

                OpCode::CoercePoint2D { dst, src } => {
                    let val = &self.stack[base + src.0 as usize];
                                        let result = val.coerce_to_point2d()?;
                                        self.stack[base + dst.0 as usize] = result;
                }

                OpCode::CoerceType { dst, src, type_name_idx } => {
                    let type_name = self.frames[frame_idx].chunk.constants[type_name_idx.0 as usize].as_compact_str()?.clone();
                    let val = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = val.coerce_to_type(type_name.as_str())?;
                }

                OpCode::BuiltinCall { builtin_id, args_start, arg_count, dst } => {
                    let args = self.stack[base + args_start.0 as usize..base + args_start.0 as usize + arg_count as usize].to_vec();
                    let res = builtins::dispatch_builtin(builtin_id, args)?;
                    self.stack[base + dst.0 as usize] = res;
                }

                OpCode::InterpolateString { dst, pattern_idx, args_start, arg_count } => {
                    let pattern = self.frames[frame_idx].chunk.constants[pattern_idx.0 as usize].as_compact_str()?.clone();
                    let mut rendered = pattern.to_string();
                    let start = base + args_start.0 as usize;
                    let end = start + arg_count as usize;
                    if end <= self.stack.len() {
                        let args = &self.stack[start..end];
                        for (i, arg) in args.iter().enumerate() {
                            let placeholder = format!("{{{}}}", i);
                            rendered = rendered.replace(&placeholder, &format!("{}", arg));
                        }
                    }
                    self.stack[base + dst.0 as usize] = Value::String(rendered.into());
                }

                // ── Native Physical Emitters (Pure Buffering with Merkle Identity) ──
                OpCode::EmitPolygon { name_reg, layer_reg, net_reg, points_or_rect_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_polygon" })?;
                    let semantic_name = match &self.stack[base + name_reg.0 as usize] {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    let layer = self.stack[base + layer_reg.0 as usize].as_compact_str()?.clone();
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };
                    let geom = &self.stack[base + points_or_rect_reg.0 as usize];
                    
                    let points = match geom {
                        Value::Array(items)
                            if items.len() == 4
                                && matches!(&items[0], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[1], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[2], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[3], Value::Measurement(_) | Value::Int(_)) =>
                        {
                            // Rect form: [x, y, w, h]
                            let x = match &items[0] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let y = match &items[1] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let w = match &items[2] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let h = match &items[3] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                        }
                        Value::Array(items) => {
                            let mut pts = Vec::new();
                            for item in items.iter() {
                                let p = item.coerce_to_point2d()?;
                                if let Value::Point2D { x, y } = p {
                                    pts.push((x, y));
                                }
                            }
                            pts
                        }
                        _ => vec![],
                    };

                    let id = EntityId::compute(
                        &self.frames[frame_idx].path,
                        "Polygon",
                        semantic_name.as_deref(),
                        self.emitted_record_count,
                    );
                    self.emitted_record_count += 1;

                    let record_size = std::mem::size_of::<GeometryRecord>() + points.len() * 16;
                    self.guard.track_allocation(record_size)?;

                    if let Some(buf) = &mut self.output_buffer {
                        buf.push(GeometryRecord::Polygon {
                            id,
                            space_id: SpaceId(space_id),
                            layer: layer.clone(),
                            net_id: net.map(|n| n.0),
                            points_pm: points.clone(),
                        });
                    }

                    self.emitter.add_polygon(space_id, layer.as_str(), net, points, semantic_name)?;
                }

                OpCode::EmitContact { name_reg, from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_contact" })?;
                    let semantic_name = match &self.stack[base + name_reg.0 as usize] {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    let from_layer = self.stack[base + from_layer_reg.0 as usize].as_compact_str()?.clone();
                    let to_layer = self.stack[base + to_layer_reg.0 as usize].as_compact_str()?.clone();
                    let at_val = self.stack[base + at_reg.0 as usize].coerce_to_point2d()?;
                    let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
                    let dia_pm = match &self.stack[base + dia_reg.0 as usize] {
                        Value::Measurement(m) => m.raw as i64,
                        Value::Int(i) => *i,
                        _ => 170_000,
                    };
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };
                    
                    let id = EntityId::compute(
                        &self.frames[frame_idx].path,
                        "Contact",
                        semantic_name.as_deref(),
                        self.emitted_record_count,
                    );
                    self.emitted_record_count += 1;

                    self.guard.track_allocation(std::mem::size_of::<GeometryRecord>())?;

                    if let Some(buf) = &mut self.output_buffer {
                        buf.push(GeometryRecord::Contact {
                            id,
                            space_id: SpaceId(space_id),
                            from_layer: from_layer.clone(),
                            to_layer: to_layer.clone(),
                            center_pm: at,
                            diameter_pm: dia_pm,
                            net_id: net.map(|n| n.0),
                        });
                    }

                    self.emitter.add_contact(space_id, from_layer.as_str(), to_layer.as_str(), at, dia_pm, net, semantic_name)?;
                }

                OpCode::EmitDevice { type_reg, name_reg, terminals_reg, params_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_device" })?;
                    let dev_type = match &self.stack[base + type_reg.0 as usize] {
                        Value::String(s) => s.clone(),
                        Value::EnumVariant { variant_name, .. } => variant_name.clone(),
                        other => CompactString::new(format!("{}", other)),
                    };
                    let name = match &self.stack[base + name_reg.0 as usize] {
                        Value::String(s) => s.clone(),
                        _ => CompactString::new("DEV"),
                    };
                    let terms = match &self.stack[base + terminals_reg.0 as usize] {
                        Value::StructInstance { fields, .. } => {
                            let mut map = FxHashMap::default();
                            for (k, v) in fields.iter() {
                                if let Value::NetHandle(id) = v {
                                    map.insert(k.clone(), *id);
                                }
                            }
                            map
                        }
                        _ => FxHashMap::default(),
                    };
                    let params = match &self.stack[base + params_reg.0 as usize] {
                        Value::StructInstance { fields, .. } => {
                            let mut map = FxHashMap::default();
                            for (k, v) in fields.iter() {
                                if let Value::Measurement(m) = v {
                                    map.insert(k.clone(), *m);
                                }
                            }
                            map
                        }
                        _ => FxHashMap::default(),
                    };
                    
                    let id = EntityId::compute(
                        &self.frames[frame_idx].path,
                        "Device",
                        Some(name.as_str()),
                        self.emitted_record_count,
                    );
                    self.emitted_record_count += 1;

                    let dev_size = std::mem::size_of::<GeometryRecord>() + terms.len() * 32 + params.len() * 32;
                    self.guard.track_allocation(dev_size)?;

                    if let Some(buf) = &mut self.output_buffer {
                        let mut term_vec = Vec::with_capacity(terms.len());
                        for (k, v) in &terms {
                            term_vec.push((k.clone(), v.0));
                        }
                        let mut param_vec = Vec::with_capacity(params.len());
                        for (k, v) in &params {
                            param_vec.push((k.clone(), v.raw as f64));
                        }
                        buf.push(GeometryRecord::Device {
                            id,
                            space_id: SpaceId(space_id),
                            device_type: dev_type.clone(),
                            instance_name: name.clone(),
                            terminals: term_vec,
                            params: param_vec,
                        });
                    }

                    self.emitter.add_device(space_id, dev_type.as_str(), name.as_str(), terms, params)?;
                }

                OpCode::EmitRoute { from_reg, to_reg, intent_idx, props_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "route" })?;
                    let from_val = self.stack[base + from_reg.0 as usize].clone();
                    let to_val = self.stack[base + to_reg.0 as usize].clone();
                    let intent = self.frames[frame_idx].chunk.constants[intent_idx.0 as usize].as_compact_str()?.clone();
                    let props = match &self.stack[base + props_reg.0 as usize] {
                        Value::StructInstance { fields, .. } => {
                            let mut map = FxHashMap::default();
                            for (k, v) in fields.iter() {
                                map.insert(k.clone(), v.clone());
                            }
                            map
                        }
                        _ => FxHashMap::default(),
                    };

                    let id = EntityId::compute(
                        &self.frames[frame_idx].path,
                        "RouteIntent",
                        Some(intent.as_str()),
                        self.emitted_record_count,
                    );
                    self.emitted_record_count += 1;

                    self.guard.track_allocation(std::mem::size_of::<GeometryRecord>())?;

                    if let Some(buf) = &mut self.output_buffer {
                        let from_port = match &from_val {
                            Value::Point2D { x, y } => (*x, *y, 0),
                            _ => (0, 0, 0),
                        };
                        let to_port = match &to_val {
                            Value::Point2D { x, y } => (*x, *y, 0),
                            _ => (0, 0, 0),
                        };
                        buf.push(GeometryRecord::RouteIntent {
                            id,
                            space_id: SpaceId(space_id),
                            from_port,
                            to_port,
                            intent: intent.clone(),
                        });
                    }

                    self.emitter.add_route(space_id, from_val, to_val, Some(intent), props)?;
                }

                OpCode::SpacePlaceCell { dst, cell_reg, at_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "place" })?;
                    let cell_val = self.stack[base + cell_reg.0 as usize].clone();
                    let at_val = self.stack[base + at_reg.0 as usize].coerce_to_point2d()?;
                    let (at_x, at_y) = match at_val {
                        Value::Point2D { x, y } => (x, y),
                        _ => (0, 0),
                    };

                    match cell_val {
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();

                            // Ingest child geometries transformed by cell.transform + placement offset
                            for poly in &cell.polygons {
                                let mut world_points = Vec::with_capacity(poly.points.len());
                                for pt in &poly.points {
                                    let (tx, ty) = cell.transform.apply_point(*pt);
                                    world_points.push((at_x + tx, at_y + ty));
                                }
                                self.emitter.add_polygon(space_id, poly.layer.as_str(), poly.net, world_points, Some(cell.name.clone()))?;
                            }

                            for c in &cell.contacts {
                                let (tx, ty) = cell.transform.apply_point(c.at);
                                let world_at = (at_x + tx, at_y + ty);
                                self.emitter.add_contact(
                                    space_id,
                                    c.from_layer.as_str(),
                                    c.to_layer.as_str(),
                                    world_at,
                                    c.diameter,
                                    c.net,
                                    Some(c.name.clone()),
                                )?;
                            }

                            for dev in &cell.devices {
                                let mut term_map = FxHashMap::default();
                                for (k, _v) in &dev.terminals {
                                    term_map.insert(k.clone(), hwc_types::NetId(0));
                                }
                                let mut param_map = FxHashMap::default();
                                for (k, v) in &dev.params {
                                    if let Value::Measurement(m) = v {
                                        param_map.insert(k.clone(), m.clone());
                                    }
                                }
                                self.emitter.add_device(space_id, dev.device_type.as_str(), dev.instance_name.as_str(), term_map, param_map)?;
                            }

                            let placed_instance = super::value::PlacedCellInstance {
                                cell,
                                placement_x: at_x,
                                placement_y: at_y,
                            };
                            self.stack[base + dst.0 as usize] = Value::PlacedCell(Arc::new(placed_instance));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellRotate { dst, cell_reg, deg_reg } => {
                    let cell_val = self.stack[base + cell_reg.0 as usize].clone();
                    let deg = match &self.stack[base + deg_reg.0 as usize] {
                        Value::Int(i) => *i as i32,
                        Value::Float(f) => *f as i32,
                        Value::Measurement(m) => (m.raw / 1_000_000) as i32,
                        _ => 0,
                    };
                    match cell_val {
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let rotated = cell.rotate(deg);
                            self.stack[base + dst.0 as usize] = Value::CellLayout(Arc::new(rotated));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellMirrorX { dst, cell_reg } => {
                    let cell_val = self.stack[base + cell_reg.0 as usize].clone();
                    match cell_val {
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let mirrored = cell.mirror_x();
                            self.stack[base + dst.0 as usize] = Value::CellLayout(Arc::new(mirrored));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellMirrorY { dst, cell_reg } => {
                    let cell_val = self.stack[base + cell_reg.0 as usize].clone();
                    match cell_val {
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let mirrored = cell.mirror_y();
                            self.stack[base + dst.0 as usize] = Value::CellLayout(Arc::new(mirrored));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellOffset { dst, cell_reg, dx_reg, dy_reg } => {
                    let cell_val = self.stack[base + cell_reg.0 as usize].clone();
                    let dx = match &self.stack[base + dx_reg.0 as usize] {
                        Value::Measurement(m) => m.raw as i64,
                        Value::Int(i) => *i,
                        _ => 0,
                    };
                    let dy = match &self.stack[base + dy_reg.0 as usize] {
                        Value::Measurement(m) => m.raw as i64,
                        Value::Int(i) => *i,
                        _ => 0,
                    };
                    match cell_val {
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let offsetted = cell.offset(dx, dy);
                            self.stack[base + dst.0 as usize] = Value::CellLayout(Arc::new(offsetted));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellPort { dst, target_reg, port_name_idx } => {
                    let target_val = self.stack[base + target_reg.0 as usize].clone();
                    let port_name = self.frames[frame_idx].chunk.constants[port_name_idx.0 as usize].as_compact_str()?.clone();

                    match target_val {
                        Value::PlacedCell(inst) => {
                            let pt = inst.port(port_name.as_str()).ok_or_else(|| EvalError::General {
                                message: format!("Port '{}' not found on placed cell '{}'", port_name, inst.cell.name),
                            })?;
                            self.stack[base + dst.0 as usize] = pt;
                        }
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let port = cell.ports.iter().find(|p| p.name == port_name).ok_or_else(|| EvalError::General {
                                message: format!("Port '{}' not found on cell '{}'", port_name, cell.name),
                            })?;
                            let (tx, ty) = cell.transform.apply_point(port.at);
                            self.stack[base + dst.0 as usize] = Value::Point2D { x: tx, y: ty };
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "PlacedCell or CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellBBox { dst, target_reg } => {
                    let target_val = self.stack[base + target_reg.0 as usize].clone();
                    match target_val {
                        Value::PlacedCell(inst) => {
                            self.stack[base + dst.0 as usize] = inst.bounding_box();
                        }
                        Value::CellLayout(cell_arc) => {
                            let cell = (*cell_arc).clone();
                            let (min_x, min_y, max_x, max_y) = cell.bounding_box();
                            self.stack[base + dst.0 as usize] = Value::BoundingBox { min_x, min_y, max_x, max_y };
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "PlacedCell or CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellNew { dst, name_reg } => {
                    let name = match &self.stack[base + name_reg.0 as usize] {
                        Value::String(s) => s.clone(),
                        _ => CompactString::new("cell"),
                    };
                    self.stack[base + dst.0 as usize] = Value::CellLayout(Arc::new(super::value::CellLayout::new(name)));
                }

                OpCode::CellAddPolygon { cell_reg, layer_reg, net_reg, rect_or_points_reg } => {
                    let layer = self.stack[base + layer_reg.0 as usize].as_compact_str()?.clone();
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };
                    let geom = &self.stack[base + rect_or_points_reg.0 as usize];
                    let points = match geom {
                        Value::Array(items)
                            if items.len() == 4
                                && matches!(&items[0], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[1], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[2], Value::Measurement(_) | Value::Int(_))
                                && matches!(&items[3], Value::Measurement(_) | Value::Int(_)) =>
                        {
                            let x = match &items[0] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let y = match &items[1] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let w = match &items[2] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            let h = match &items[3] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                        }
                        Value::Array(items) => {
                            let mut pts = Vec::new();
                            for item in items.iter() {
                                let p = item.coerce_to_point2d()?;
                                if let Value::Point2D { x, y } = p {
                                    pts.push((x, y));
                                }
                            }
                            pts
                        }
                        _ => vec![],
                    };

                    match &mut self.stack[base + cell_reg.0 as usize] {
                        Value::CellLayout(cell_arc) => {
                            Arc::make_mut(cell_arc).add_polygon(layer, points, net);
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellAddContact { cell_reg, from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg } => {
                    let from_layer = self.stack[base + from_layer_reg.0 as usize].as_compact_str()?.clone();
                    let to_layer = self.stack[base + to_layer_reg.0 as usize].as_compact_str()?.clone();
                    let at_val = self.stack[base + at_reg.0 as usize].coerce_to_point2d()?;
                    let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
                    let dia_pm = match &self.stack[base + dia_reg.0 as usize] {
                        Value::Measurement(m) => m.raw as i64,
                        Value::Int(i) => *i,
                        _ => 170_000,
                    };
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };

                    match &mut self.stack[base + cell_reg.0 as usize] {
                        Value::CellLayout(cell_arc) => {
                            Arc::make_mut(cell_arc).add_contact(from_layer, to_layer, at, dia_pm, None, net);
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellAddPort { cell_reg, name_reg, at_reg, layer_reg, net_reg } => {
                    let name = self.stack[base + name_reg.0 as usize].as_compact_str()?.clone();
                    let at_val = self.stack[base + at_reg.0 as usize].coerce_to_point2d()?;
                    let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
                    let layer = self.stack[base + layer_reg.0 as usize].as_compact_str()?.clone();
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };

                    match &mut self.stack[base + cell_reg.0 as usize] {
                        Value::CellLayout(cell_arc) => {
                            Arc::make_mut(cell_arc).add_port(name, at, layer, net);
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellAddDevice { cell_reg, type_reg, terms_reg, params_reg } => {
                    let dev_type = match &self.stack[base + type_reg.0 as usize] {
                        Value::String(s) => s.clone(),
                        Value::EnumVariant { variant_name, .. } => variant_name.clone(),
                        other => CompactString::new(format!("{}", other)),
                    };
                    let terms = match &self.stack[base + terms_reg.0 as usize] {
                        Value::StructInstance { fields, .. } => {
                            let mut vec = Vec::new();
                            for (k, v) in fields.iter() {
                                let term_str = match v {
                                    Value::String(s) => s.clone(),
                                    other => CompactString::new(format!("{}", other)),
                                };
                                vec.push((k.clone(), term_str));
                            }
                            vec
                        }
                        _ => Vec::new(),
                    };
                    let params = match &self.stack[base + params_reg.0 as usize] {
                        Value::StructInstance { fields, .. } => {
                            let mut vec = Vec::new();
                            for (k, v) in fields.iter() {
                                vec.push((k.clone(), v.clone()));
                            }
                            vec
                        }
                        _ => Vec::new(),
                    };

                    match &mut self.stack[base + cell_reg.0 as usize] {
                        Value::CellLayout(cell_arc) => {
                            let cell_name = cell_arc.name.clone();
                            Arc::make_mut(cell_arc).add_device(dev_type, cell_name, terms, params);
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CellPlace { cell_reg, child_cell_reg, at_reg } => {
                    let at_val = self.stack[base + at_reg.0 as usize].coerce_to_point2d()?;
                    let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
                    let child_val = self.stack[base + child_cell_reg.0 as usize].clone();

                    match (&mut self.stack[base + cell_reg.0 as usize], child_val) {
                        (Value::CellLayout(cell_arc), Value::CellLayout(child_arc)) => {
                            Arc::make_mut(cell_arc).place(&child_arc, at);
                        }
                        (other, _) => return Err(EvalError::TypeMismatch {
                            expected: "CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::CallMethod { method_name_idx, target_reg, args_start, arg_count, dst } => {
                    let method_name = self.frames[frame_idx].chunk.constants[method_name_idx.0 as usize].as_compact_str()?.clone();
                    let target_val = self.stack[base + target_reg.0 as usize].clone();

                    match &target_val {
                        Value::BoundingBox { min_x, min_y, max_x, max_y } => {
                            match method_name.as_str() {
                                "width" => {
                                    self.stack[base + dst.0 as usize] = Value::Measurement(MeasurementValue::length_pm((max_x - min_x) as i128));
                                }
                                "height" => {
                                    self.stack[base + dst.0 as usize] = Value::Measurement(MeasurementValue::length_pm((max_y - min_y) as i128));
                                }
                                "center" => {
                                    self.stack[base + dst.0 as usize] = Value::Point2D {
                                        x: (min_x + max_x) / 2,
                                        y: (min_y + max_y) / 2,
                                    };
                                }
                                _ => return Err(EvalError::UnknownFunction { name: method_name }),
                            }
                        }
                        Value::StructInstance { name: struct_name, .. } => {
                            let qualified_name: CompactString = format!("{}::{}", struct_name, method_name).into();
                            let chunk = self.functions.get(&qualified_name).cloned().ok_or_else(|| {
                                EvalError::UnknownFunction { name: qualified_name.clone() }
                            })?;

                            // Build arguments with target as self (first arg)
                            let mut full_args = vec![target_val];
                            let start = base + args_start.0 as usize;
                            let end = start + arg_count as usize;
                            if end <= self.stack.len() {
                                full_args.extend_from_slice(&self.stack[start..end]);
                            }

                            // Setup call frame
                            if self.frames.len() >= MAX_CALL_STACK_DEPTH {
                                return Err(EvalError::RecursionDepthExceeded(MAX_CALL_STACK_DEPTH));
                            }
                            let mut child_path = self.frames[frame_idx].path.clone();
                            child_path.push(PathSegment::Instance(qualified_name.clone()));
                            let child_base = self.stack.len();
                            let needed = (chunk.max_registers as usize).max(full_args.len()).max(32);
                            self.stack.resize(child_base + needed, Value::Void);
                            for (i, arg) in full_args.into_iter().enumerate() {
                                self.stack[child_base + i] = arg;
                            }
                            self.frames.push(CallFrame {
                                chunk,
                                ip: 0,
                                stack_base: child_base,
                                return_register: Some(dst),
                                function_name: qualified_name.clone(),
                                path: child_path,
                            });
                            continue;
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "StructInstance, BoundingBox, or CellLayout",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::Assert { cond, msg_idx } => {
                    let c = match &self.stack[base + cond.0 as usize] {
                        Value::Bool(b) => *b,
                        Value::Int(i) => *i != 0,
                        _ => false,
                    };
                    if !c {
                        let msg = self.frames[frame_idx].chunk.constants[msg_idx.0 as usize].as_compact_str()?.clone();
                        return Err(EvalError::AssertionFailed { message: msg.to_string() });
                    }
                }

                // ── NEW: Compound & Local Register Arithmetic ──
                OpCode::AddAssign { dst, src } => {
                    let l = self.stack[base + dst.0 as usize].clone();
                    let r = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = l.add(r)?;
                }

                OpCode::SubAssign { dst, src } => {
                    let l = self.stack[base + dst.0 as usize].clone();
                    let r = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = l.sub(r)?;
                }

                OpCode::MulAssign { dst, src } => {
                    let l = self.stack[base + dst.0 as usize].clone();
                    let r = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = l.mul(r)?;
                }

                OpCode::DivAssign { dst, src } => {
                    let l = self.stack[base + dst.0 as usize].clone();
                    let r = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = l.div(r)?;
                }

                OpCode::ModAssign { dst, src } => {
                    let l = self.stack[base + dst.0 as usize].clone();
                    let r = &self.stack[base + src.0 as usize];
                    self.stack[base + dst.0 as usize] = l.modulo(r)?;
                }

                // ── NEW: Control Flow Jumps ──
                OpCode::JumpForward { offset } => {
                    let frame = &mut self.frames[frame_idx];
                    frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                }

                OpCode::JumpBack { offset } => {
                    let frame = &mut self.frames[frame_idx];
                    frame.ip = (frame.ip as i32 - offset.0 - 1) as usize;
                }

                // ── NEW: Array & Collection Operations ──
                OpCode::ArrayPush { array_reg, val_reg } => {
                    let val = self.stack[base + val_reg.0 as usize].clone();
                    let arr_val = &mut self.stack[base + array_reg.0 as usize];
                    match arr_val {
                        Value::Array(ref mut arc_vec) => {
                            Arc::make_mut(arc_vec).push(val);
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Array",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                OpCode::ArrayPop { dst, array_reg } => {
                    let arr_val = &mut self.stack[base + array_reg.0 as usize];
                    let popped_val = match arr_val {
                        Value::Array(ref mut arc_vec) => {
                            Arc::make_mut(arc_vec).pop().unwrap_or(Value::Void)
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Array",
                            found: other.type_name().to_string(),
                        }),
                    };
                    self.stack[base + dst.0 as usize] = popped_val;
                }

                OpCode::ArrayLen { dst, array_reg } => {
                    let arr_val = &self.stack[base + array_reg.0 as usize];
                    let len = match arr_val {
                        Value::Array(items) => items.len() as i64,
                        Value::String(s) => s.len() as i64,
                        Value::Tuple(items) => items.len() as i64,
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Array, String, or Tuple",
                            found: other.type_name().to_string(),
                        }),
                    };
                    self.stack[base + dst.0 as usize] = Value::Int(len);
                }

                OpCode::ArraySlice { dst, array_reg, start_reg, end_reg } => {
                    let arr_val = &self.stack[base + array_reg.0 as usize];
                    let start_val = &self.stack[base + start_reg.0 as usize];
                    let end_val = &self.stack[base + end_reg.0 as usize];

                    match arr_val {
                        Value::Array(items) => {
                            let len = items.len() as i64;
                            let start = match start_val {
                                Value::Int(i) => (*i).max(0).min(len) as usize,
                                Value::Void => 0,
                                _ => 0,
                            };
                            let end = match end_val {
                                Value::Int(i) => (*i).max(0).min(len) as usize,
                                Value::Void => items.len(),
                                _ => items.len(),
                            };
                            let slice_items = if start <= end && start <= items.len() {
                                items[start..end.min(items.len())].to_vec()
                            } else {
                                Vec::new()
                            };
                            self.stack[base + dst.0 as usize] = Value::Array(Arc::new(slice_items));
                        }
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Array",
                            found: other.type_name().to_string(),
                        }),
                    }
                }

                // ── NEW: Tuple & Destructuring Ops ──
                OpCode::AllocTuple { dst, start_reg, count } => {
                    let mut elements = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        elements.push(self.stack[base + start_reg.0 as usize + i as usize].clone());
                    }
                    self.stack[base + dst.0 as usize] = Value::Tuple(Arc::new(elements));
                }

                OpCode::UnpackTuple { dst_start, tuple_reg, count } => {
                    // Clone the values out first to release the immutable borrow
                    // before writing into destination stack slots.
                    let unpacked: Vec<Value> = {
                        let tuple_val = &self.stack[base + tuple_reg.0 as usize];
                        match tuple_val {
                            Value::Tuple(items) | Value::Array(items) => {
                                (0..count)
                                    .map(|i| items.get(i as usize).cloned().unwrap_or(Value::Void))
                                    .collect()
                            }
                            other => {
                                let first = other.clone();
                                (0..count)
                                    .map(|i| if i == 0 { first.clone() } else { Value::Void })
                                    .collect()
                            }
                        }
                    };
                    for (i, v) in unpacked.into_iter().enumerate() {
                        self.stack[base + dst_start.0 as usize + i] = v;
                    }
                }

                // ── NEW: Unit Conversion Ops ──
                OpCode::MeasToFloat { dst, src } => {
                    let val = &self.stack[base + src.0 as usize];
                    let float_val = match val {
                        Value::Measurement(m) => {
                            let scale = m.dimension.si_to_internal_scale();
                            m.raw as f64 / scale
                        }
                        Value::Float(f) => *f,
                        Value::Int(i) => *i as f64,
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Measurement, Float, or Int",
                            found: other.type_name().to_string(),
                        }),
                    };
                    self.stack[base + dst.0 as usize] = Value::Float(float_val);
                }

                OpCode::MeasToInt { dst, src } => {
                    let val = &self.stack[base + src.0 as usize];
                    let int_val = match val {
                        Value::Measurement(m) => m.raw as i64,
                        Value::Int(i) => *i,
                        Value::Float(f) => *f as i64,
                        other => return Err(EvalError::TypeMismatch {
                            expected: "Measurement, Int, or Float",
                            found: other.type_name().to_string(),
                        }),
                    };
                    self.stack[base + dst.0 as usize] = Value::Int(int_val);
                }
            }
        }

        Ok(Value::Void)
    }

    fn eval_cmp<F>(&self, left: &Value, right: &Value, cmp: F) -> Result<Value, EvalError>
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
