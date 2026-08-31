use compact_str::CompactString;
use hwc_parser::ast::Span;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_variable(&mut self, name: &CompactString, span: Span) -> Result<Register, EvalError> {
        if let Some((src_reg, _)) = self.lookup_var(name.as_str()) {
            return Ok(src_reg);
        }

        if let Some(enum_value) = self.enum_types.get(name.as_str()).cloned() {
            let dst = self.alloc_reg();
            let const_idx = self.chunk.add_constant(enum_value);
            self.chunk.emit(OpCode::LoadConst { dst, const_idx }, span);
            return Ok(dst);
        }

        Err(EvalError::UndefinedVariable { name: name.clone() })
    }
}
