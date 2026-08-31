use compact_str::CompactString;
use hwc_parser::ast::{Expression, Span};

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::Value;

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_field_access(
        &mut self,
        target: &Expression,
        field: &CompactString,
        span: Span,
    ) -> Result<Register, EvalError> {
        let obj = self.compile_expression(target)?;
        let field_idx = self.chunk.add_constant(Value::String(field.clone()));
        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::GetField {
                dst,
                obj,
                field_idx,
            },
            span,
        );
        Ok(dst)
    }

    pub(super) fn compile_index_access(
        &mut self,
        target: &Expression,
        index: &Expression,
        span: Span,
    ) -> Result<Register, EvalError> {
        let obj = self.compile_expression(target)?;
        let idx_reg = self.compile_expression(index)?;
        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::GetIndex {
                dst,
                obj,
                index: idx_reg,
            },
            span,
        );
        Ok(dst)
    }
}
