use hwc_parser::ast::Span;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::Value;

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_interpolated_string(
        &mut self,
        pattern: &str,
        expr_strs: &[String],
        span: Span,
    ) -> Result<Register, EvalError> {
        let mut arg_regs = Vec::new();
        for expr_str in expr_strs {
            let lexer = hwc_parser::Lexer::new(expr_str);
            if let Ok(tokens) = lexer.tokenize() {
                let mut parser = hwc_parser::Parser::new(tokens);
                if let Ok(sub_expr) = parser.parse_expression() {
                    let r = self.compile_expression(&sub_expr)?;
                    arg_regs.push(r);
                    continue;
                }
            }
            if let Some((src_reg, _)) = self.lookup_var(expr_str.trim()) {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::Move { dst, src: src_reg }, span);
                arg_regs.push(dst);
            } else {
                let dst = self.alloc_reg();
                let const_idx = self.chunk.add_constant(Value::String(expr_str.clone().into()));
                self.chunk.emit(OpCode::LoadConst { dst, const_idx }, span);
                arg_regs.push(dst);
            }
        }

        let start_reg = self.alloc_reg();
        if let Some(&first) = arg_regs.first() {
            self.chunk.emit(OpCode::Move { dst: start_reg, src: first }, span);
            for &r in &arg_regs[1..] {
                let next = self.alloc_reg();
                self.chunk.emit(OpCode::Move { dst: next, src: r }, span);
            }
        }

        let pattern_idx = self.chunk.add_constant(Value::String(pattern.into()));
        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::InterpolateString {
                dst,
                pattern_idx,
                args_start: start_reg,
                arg_count: arg_regs.len() as u8,
            },
            span,
        );
        Ok(dst)
    }
}
