use hwc_parser::ast::{Span, Unit};

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::{MeasurementValue, Value};

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_measurement(
        &mut self,
        value: f64,
        unit: &Unit,
        span: Span,
    ) -> Result<Register, EvalError> {
        let dst = self.alloc_reg();
        let val = if let Some(m) = MeasurementValue::from_ast_unit(value, unit, self.unit_registry) {
            Value::Measurement(m)
        } else {
            Value::Float(value)
        };
        let const_idx = self.chunk.add_constant(val);
        self.chunk.emit(OpCode::LoadConst { dst, const_idx }, span);
        Ok(dst)
    }
}
