use compact_str::CompactString;
use hwc_parser::ast::{FieldInit, Span, TypeExpr};
use std::sync::Arc;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::Value;

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_struct_instance(
        &mut self,
        name: &CompactString,
        fields: &[FieldInit],
        span: Span,
    ) -> Result<Register, EvalError> {
        let struct_decl = self.struct_decls.get(name.as_str()).cloned();
        let mut field_names = Vec::new();
        let mut field_regs = Vec::new();

        for field in fields {
            field_names.push(field.name.clone());
            let raw_val_r = if let Some(v_expr) = &field.value {
                self.compile_expression(v_expr)?
            } else {
                let (src_reg, _) = self.lookup_var(field.name.as_str()).ok_or_else(|| {
                    EvalError::UndefinedVariable { name: field.name.clone() }
                })?;
                src_reg
            };

            let val_r = if let Some(decl) = &struct_decl {
                if let Some(decl_field) = decl.fields.iter().find(|f| f.name == field.name) {
                    if let TypeExpr::Named { name: type_name, .. } = &decl_field.type_annotation {
                        if type_name.as_str() == "Point2D" {
                            let coerced_reg = self.alloc_reg();
                            self.chunk.emit(
                                OpCode::CoercePoint2D {
                                    dst: coerced_reg,
                                    src: raw_val_r,
                                },
                                span,
                            );
                            coerced_reg
                        } else {
                            raw_val_r
                        }
                    } else {
                        raw_val_r
                    }
                } else {
                    raw_val_r
                }
            } else {
                raw_val_r
            };

            field_regs.push(val_r);
        }

        let start_reg = self.alloc_reg();
        for (i, r) in field_regs.iter().enumerate() {
            let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
            self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
        }

        let struct_meta = Value::StructInstance {
            name: name.clone(),
            fields: Arc::new(field_names.into_iter().map(|n| (n, Value::Void)).collect()),
        };
        let struct_const = self.chunk.add_constant(struct_meta);

        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::AllocStruct {
                dst,
                struct_name_idx: struct_const,
                fields_start: start_reg,
                count: field_regs.len() as u16,
            },
            span,
        );
        Ok(dst)
    }
}
