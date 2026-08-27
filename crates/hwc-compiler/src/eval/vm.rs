//! HardwareScript v0.3.0 Bytecode Virtual Machine (`hwc-eval`)
//!
//! Executes linear bytecode chunks on a flat activation stack with static activation records.

use compact_str::CompactString;
use hwc_types::UnitRegistry;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::builtins;
use super::context::EvalError;
use super::emitter::SpaceEmitter;
use super::frame::CallFrame;
use super::opcodes::{Chunk, OpCode};
use super::sandbox::{MAX_EVAL_STEPS, MAX_RECURSION_DEPTH};
use super::value::{MeasurementValue, Value};

/// Bytecode Virtual Machine
pub struct VM<'a> {
    pub stack: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub functions: FxHashMap<CompactString, Arc<Chunk>>,
    pub step_count: usize,
    pub max_steps: usize,
    pub current_space_id: Option<u32>,
    pub emitter: &'a mut dyn SpaceEmitter,
    pub unit_registry: Option<Arc<UnitRegistry>>,
}

impl<'a> VM<'a> {
    pub fn new(emitter: &'a mut dyn SpaceEmitter) -> Self {
        Self {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(256),
            functions: FxHashMap::default(),
            step_count: 0,
            max_steps: MAX_EVAL_STEPS,
            current_space_id: None,
            emitter,
            unit_registry: None,
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
        eprintln!("[VM DEBUG] Starting execution of chunk '{}' (code len: {}, consts: {})", chunk.name, chunk.code.len(), chunk.constants.len());
        self.current_space_id = space_id;

        let stack_base = self.stack.len();
        let num_regs = (chunk.max_registers as usize).max(64);
        self.stack.resize(stack_base + num_regs, Value::Void);

        self.frames.push(CallFrame::new(
            chunk,
            stack_base,
            None,
            "main",
        ));

        self.run()
    }

    /// Main instruction dispatch loop
    pub fn run(&mut self) -> Result<Value, EvalError> {
        while !self.frames.is_empty() {
            self.step_count += 1;
            if self.step_count > self.max_steps {
                return Err(EvalError::StepLimitExceeded(self.max_steps));
            }

            let frame_idx = self.frames.len() - 1;

            let (op, base) = {
                let frame = &mut self.frames[frame_idx];
                if frame.ip >= frame.chunk.code.len() {
                    let popped = self.frames.pop().unwrap();
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
                    if self.frames.len() >= MAX_RECURSION_DEPTH {
                        return Err(EvalError::RecursionDepthExceeded(MAX_RECURSION_DEPTH));
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

                    self.frames.push(CallFrame::new(
                        target_chunk,
                        new_base,
                        Some(dst),
                        func_name,
                    ));
                }

                OpCode::Return { val } => {
                    let return_val = self.stack[base + val.0 as usize].clone();
                    let popped = self.frames.pop().unwrap();
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
                    eprintln!("[VM DEBUG] GetField: field='{}' from type={} (value: {:?})", field_name, target_obj.type_name(), target_obj);
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
                            eprintln!("[VM DEBUG] GetField result: {:?}", val.type_name());
                            self.stack[base + dst.0 as usize] = val;
                        }
                        Value::EnumType { name, variants } => {
                            let val = variants.get(&field_name).cloned().ok_or_else(|| {
                                EvalError::FieldNotFound {
                                    field: field_name.clone(),
                                    struct_name: name.clone(),
                                }
                            })?;
                            eprintln!("[VM DEBUG] GetField result: {:?}", val.type_name());
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
                            eprintln!("[VM DEBUG] GetField result from Point2D.{}: {:?}", field_name, result.type_name());
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
                            eprintln!("[VM DEBUG] GetField result from Point3D.{}: {:?}", field_name, result.type_name());
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
                            eprintln!("[VM DEBUG] GetField result from Vector2D.{}: {:?}", field_name, result.type_name());
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
                            eprintln!("[VM DEBUG] GetField result from BoundingBox.{}: {:?}", field_name, result.type_name());
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
                        (Value::Array(items), Value::Int(i)) => {
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
                    eprintln!("[VM DEBUG] CoercePoint2D: input type={:?}", val.type_name());
                    let result = val.coerce_to_point2d()?;
                    eprintln!("[VM DEBUG] CoercePoint2D: output type={:?}", result.type_name());
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

                // ── Native Physical Emitters ──
                OpCode::EmitPolygon { layer_reg, net_reg, points_or_rect_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_polygon" })?;
                    let layer = self.stack[base + layer_reg.0 as usize].as_compact_str()?.clone();
                    let net = match &self.stack[base + net_reg.0 as usize] {
                        Value::NetHandle(id) => Some(*id),
                        _ => None,
                    };
                    let geom = &self.stack[base + points_or_rect_reg.0 as usize];
                    eprintln!("[VM DEBUG] *** VM EMIT POLYGON: layer='{}', net={:?}, geom={:?}", layer, net, geom);

                    let points = match geom {
                        Value::Array(items) if items.len() == 4 => {
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

                    self.emitter.add_polygon(space_id, layer.as_str(), net, points, None)?;
                }

                OpCode::EmitContact { from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg } => {
                    let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_contact" })?;
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
                    eprintln!("[VM DEBUG] *** VM EMIT CONTACT: from='{}', to='{}', at={:?}, dia={}pm, net={:?}", from_layer, to_layer, at, dia_pm, net);
                    self.emitter.add_contact(space_id, from_layer.as_str(), to_layer.as_str(), at, dia_pm, net, None)?;
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
                    eprintln!("[VM DEBUG] *** VM EMIT DEVICE: type='{}', name='{}', terms={:?}, params={:?}", dev_type, name, terms, params);
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
                    self.emitter.add_route(space_id, from_val, to_val, Some(intent), props)?;
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
