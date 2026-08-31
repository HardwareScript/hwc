use hwc_parser::ast::{Expression, Span};

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_array_literal(
        &mut self,
        elements: &[Expression],
        span: Span,
    ) -> Result<Register, EvalError> {
        let mut elem_regs = Vec::new();
        for elem in elements {
            elem_regs.push(self.compile_expression(elem)?);
        }

        let dst = self.alloc_reg();
        if elem_regs.is_empty() {
            let start_reg = self.alloc_reg();
            self.chunk.emit(
                OpCode::AllocArray {
                    dst,
                    start_reg,
                    count: 0,
                },
                span,
            );
        } else {
            let start_reg = self.alloc_reg();
            for (i, reg) in elem_regs.iter().enumerate() {
                let target_r = if i == 0 {
                    start_reg
                } else {
                    self.alloc_reg()
                };
                self.chunk.emit(
                    OpCode::Move {
                        dst: target_r,
                        src: *reg,
                    },
                    span,
                );
            }
            self.chunk.emit(
                OpCode::AllocArray {
                    dst,
                    start_reg,
                    count: elem_regs.len() as u16,
                },
                span,
            );
        }
        Ok(dst)
    }

    pub(super) fn compile_tuple(
        &mut self,
        elements: &[Expression],
        span: Span,
    ) -> Result<Register, EvalError> {
        let mut elem_regs = Vec::new();
        for elem in elements {
            elem_regs.push(self.compile_expression(elem)?);
        }
        let start_reg = self.alloc_reg();
        for (i, r) in elem_regs.iter().enumerate() {
            let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
            self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
        }
        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::AllocTuple {
                dst,
                start_reg,
                count: elements.len() as u8,
            },
            span,
        );
        Ok(dst)
    }

    pub(super) fn compile_slice(
        &mut self,
        target: &Expression,
        start: Option<&Expression>,
        end: Option<&Expression>,
        inclusive: bool,
        span: Span,
    ) -> Result<Register, EvalError> {
        let array_reg = self.compile_expression(target)?;
        let start_reg = if let Some(s) = start {
            self.compile_expression(s)?
        } else {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        };

        let end_reg = if let Some(e) = end {
            let r = self.compile_expression(e)?;
            if inclusive {
                let one_reg = self.alloc_reg();
                self.chunk.emit(OpCode::LoadInt { dst: one_reg, val: 1 }, span);
                let inc_reg = self.alloc_reg();
                self.chunk.emit(OpCode::Add { dst: inc_reg, lhs: r, rhs: one_reg }, span);
                inc_reg
            } else {
                r
            }
        } else {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        };

        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::ArraySlice {
                dst,
                array_reg,
                start_reg,
                end_reg,
            },
            span,
        );
        Ok(dst)
    }
}
