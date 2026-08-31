use compact_str::CompactString;
use hwc_engine::entity_graph::identity::PathSegment;

use super::super::super::context::EvalError;
use super::super::super::frame::CallFrame;
use super::super::super::sandbox::MAX_CALL_STACK_DEPTH;
use super::super::super::value::{MeasurementValue, Value};
use super::super::super::opcodes::Register;
use super::super::vm_core::VM;

impl<'a> VM<'a> {
    pub(crate) fn handle_call_method(
        &mut self,
        frame_idx: usize,
        base: usize,
        method_name_idx: u16,
        target_reg: u16,
        args_start: u16,
        arg_count: u8,
        dst: u16,
    ) -> Result<bool, EvalError> {
        let method_name = self.frames[frame_idx].chunk.constants[method_name_idx as usize].as_compact_str()?.clone();
        let target_val = self.stack[base + target_reg as usize].clone();

        match &target_val {
            Value::BoundingBox { min_x, min_y, max_x, max_y } => {
                match method_name.as_str() {
                    "width" => {
                        self.stack[base + dst as usize] = Value::Measurement(MeasurementValue::length_pm((max_x - min_x) as i128));
                    }
                    "height" => {
                        self.stack[base + dst as usize] = Value::Measurement(MeasurementValue::length_pm((max_y - min_y) as i128));
                    }
                    "center" => {
                        self.stack[base + dst as usize] = Value::Point2D {
                            x: (min_x + max_x) / 2,
                            y: (min_y + max_y) / 2,
                        };
                    }
                    _ => return Err(EvalError::UnknownFunction { name: method_name }),
                }
                Ok(false)
            }
            Value::StructInstance { name: struct_name, .. } => {
                let qualified_name: CompactString = format!("{}::{}", struct_name, method_name).into();
                let chunk = self.functions.get(&qualified_name).cloned().ok_or_else(|| {
                    EvalError::UnknownFunction { name: qualified_name.clone() }
                })?;

                let mut full_args = vec![target_val];
                let start = base + args_start as usize;
                let end = start + arg_count as usize;
                if end <= self.stack.len() {
                    full_args.extend_from_slice(&self.stack[start..end]);
                }

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
                    return_register: Some(Register(dst)),
                    function_name: qualified_name.clone(),
                    path: child_path,
                });
                Ok(true)
            }
            other => Err(EvalError::TypeMismatch {
                expected: "StructInstance, BoundingBox, or CellLayout",
                found: other.type_name().to_string(),
            }),
        }
    }
}
