use hwc_engine::entity_graph::identity::PathSegment;
use std::sync::Arc;

use super::super::builtins;
use super::super::context::EvalError;
use super::super::frame::CallFrame;
use super::super::opcodes::OpCode;
use super::super::sandbox::{SandboxError, MAX_CALL_STACK_DEPTH};
use super::super::value::Value;
use super::vm_core::VM;

impl<'a> VM<'a> {
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
                // ── Load / Move ──
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

                // ── Arithmetic ──
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

                // ── Comparison / Logic ──
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

                // ── Bitwise / Shift ──
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

                // ── Control Flow ──
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
                OpCode::JumpForward { offset } => {
                    let frame = &mut self.frames[frame_idx];
                    frame.ip = (frame.ip as i32 + offset.0 - 1) as usize;
                }
                OpCode::JumpBack { offset } => {
                    let frame = &mut self.frames[frame_idx];
                    frame.ip = (frame.ip as i32 - offset.0 - 1) as usize;
                }

                // ── Call / Return ──
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
                    for i in 0..arg_count {
                        let arg_val = self.stack[base + args_start.0 as usize + i as usize].clone();
                        self.stack.push(arg_val);
                    }
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

                // ── Alloc ──
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
                OpCode::AllocTuple { dst, start_reg, count } => {
                    let mut elements = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        elements.push(self.stack[base + start_reg.0 as usize + i as usize].clone());
                    }
                    self.stack[base + dst.0 as usize] = Value::Tuple(Arc::new(elements));
                }

                // ── Field / Index Access ──
                OpCode::GetField { dst, obj, field_idx } => {
                    self.handle_get_field(frame_idx, base, dst.0, obj.0, field_idx.0)?;
                }
                OpCode::SetField { obj, field_idx, src } => {
                    self.handle_set_field(frame_idx, base, obj.0, field_idx.0, src.0)?;
                }
                OpCode::GetIndex { dst, obj, index } => {
                    self.handle_get_index(base, dst.0, obj.0, index.0)?;
                }

                // ── Coercion ──
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

                // ── Builtin ──
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

                // ── Emit ──
                OpCode::EmitPolygon { name_reg, layer_reg, net_reg, points_or_rect_reg } => {
                    self.handle_emit_polygon(frame_idx, base, name_reg.0, layer_reg.0, net_reg.0, points_or_rect_reg.0)?;
                }
                OpCode::EmitContact { name_reg, from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg } => {
                    self.handle_emit_contact(frame_idx, base, name_reg.0, from_layer_reg.0, to_layer_reg.0, at_reg.0, dia_reg.0, net_reg.0)?;
                }
                OpCode::EmitDevice { type_reg, name_reg, terminals_reg, params_reg } => {
                    self.handle_emit_device(frame_idx, base, type_reg.0, name_reg.0, terminals_reg.0, params_reg.0)?;
                }
                OpCode::EmitRoute { from_reg, to_reg, intent_idx, props_reg } => {
                    self.handle_emit_route(frame_idx, base, from_reg.0, to_reg.0, intent_idx.0, props_reg.0)?;
                }

                // ── Cell Operations ──
                OpCode::SpacePlaceCell { dst, cell_reg, at_reg } => {
                    self.handle_space_place_cell(frame_idx, base, dst.0, cell_reg.0, at_reg.0)?;
                }
                OpCode::CellRotate { dst, cell_reg, deg_reg } => {
                    self.handle_cell_rotate(base, dst.0, cell_reg.0, deg_reg.0)?;
                }
                OpCode::CellMirrorX { dst, cell_reg } => {
                    self.handle_cell_mirror_x(base, dst.0, cell_reg.0)?;
                }
                OpCode::CellMirrorY { dst, cell_reg } => {
                    self.handle_cell_mirror_y(base, dst.0, cell_reg.0)?;
                }
                OpCode::CellOffset { dst, cell_reg, dx_reg, dy_reg } => {
                    self.handle_cell_offset(base, dst.0, cell_reg.0, dx_reg.0, dy_reg.0)?;
                }
                OpCode::CellPort { dst, target_reg, port_name_idx } => {
                    self.handle_cell_port(frame_idx, base, dst.0, target_reg.0, port_name_idx.0)?;
                }
                OpCode::CellBBox { dst, target_reg } => {
                    self.handle_cell_bbox(base, dst.0, target_reg.0)?;
                }
                OpCode::CellNew { dst, name_reg } => {
                    self.handle_cell_new(base, dst.0, name_reg.0)?;
                }
                OpCode::CellAddPolygon { cell_reg, layer_reg, net_reg, rect_or_points_reg, port_reg } => {
                    self.handle_cell_add_polygon(base, cell_reg.0, layer_reg.0, net_reg.0, rect_or_points_reg.0, port_reg.0)?;
                }
                OpCode::CellAddContact { cell_reg, from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg } => {
                    self.handle_cell_add_contact(base, cell_reg.0, from_layer_reg.0, to_layer_reg.0, at_reg.0, dia_reg.0, net_reg.0)?;
                }
                OpCode::CellAddPort { cell_reg, name_reg, at_reg, layer_reg, net_reg } => {
                    self.handle_cell_add_port(base, cell_reg.0, name_reg.0, at_reg.0, layer_reg.0, net_reg.0)?;
                }
                OpCode::CellAddDevice { cell_reg, type_reg, terms_reg, params_reg } => {
                    self.handle_cell_add_device(base, cell_reg.0, type_reg.0, terms_reg.0, params_reg.0)?;
                }
                OpCode::CellPlace { cell_reg, child_cell_reg, at_reg } => {
                    self.handle_cell_place(base, cell_reg.0, child_cell_reg.0, at_reg.0)?;
                }

                // ── Method Dispatch ──
                OpCode::CallMethod { method_name_idx, target_reg, args_start, arg_count, dst } => {
                    let should_continue = self.handle_call_method(frame_idx, base, method_name_idx.0, target_reg.0, args_start.0, arg_count, dst.0)?;
                    if should_continue {
                        continue;
                    }
                }

                // ── Assert ──
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

                // ── Compound Assign ──
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

                // ── Array Operations ──
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

                // ── Tuple Operations ──
                OpCode::UnpackTuple { dst_start, tuple_reg, count } => {
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

                // ── Unit Conversion ──
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
}
