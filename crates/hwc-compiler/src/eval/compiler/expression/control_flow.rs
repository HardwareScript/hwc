use hwc_parser::ast::{Block, ElseBranchExpr, Expression, MatchArmBody, MatchArmExpr, Pattern, Span};

use crate::eval::context::EvalError;
use crate::eval::opcodes::{JumpOffset, OpCode, Register};

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_block_expr(
        &mut self,
        block: &Block,
        span: Span,
    ) -> Result<Register, EvalError> {
        self.push_scope();
        for s in &block.statements {
            self.compile_statement(s)?;
        }
        let res_reg = if let Some(tail) = &block.tail_expr {
            self.compile_expression(tail)?
        } else {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        };
        self.pop_scope();
        Ok(res_reg)
    }

    pub(super) fn compile_if_expr(
        &mut self,
        condition: &Expression,
        then_branch: &Block,
        else_branch: Option<&ElseBranchExpr>,
        span: Span,
    ) -> Result<Register, EvalError> {
        let cond_reg = self.compile_expression(condition)?;
        let jump_false_idx = self.chunk.emit(
            OpCode::JumpIfFalse {
                cond: cond_reg,
                offset: JumpOffset(0),
            },
            span,
        );

        let result_reg = self.alloc_reg();

        let then_res = self.compile_block_expr(then_branch, span)?;
        self.chunk.emit(OpCode::Move { dst: result_reg, src: then_res }, span);

        let jump_exit_idx = self.chunk.emit(
            OpCode::Jump {
                offset: JumpOffset(0),
            },
            span,
        );

        let else_start = self.chunk.code.len();
        let offset = else_start as i32 - jump_false_idx as i32;
        self.chunk.code[jump_false_idx] = OpCode::JumpIfFalse {
            cond: cond_reg,
            offset: JumpOffset(offset),
        };

        if let Some(else_br) = else_branch {
            match else_br {
                ElseBranchExpr::ElseIf(expr) => {
                    let else_res = self.compile_expression(expr)?;
                    self.chunk.emit(OpCode::Move { dst: result_reg, src: else_res }, span);
                }
                ElseBranchExpr::Block(b) => {
                    let else_res = self.compile_block_expr(b, span)?;
                    self.chunk.emit(OpCode::Move { dst: result_reg, src: else_res }, span);
                }
            }
        } else {
            self.chunk.emit(OpCode::LoadNull { dst: result_reg }, span);
        }

        let end_pos = self.chunk.code.len();
        let exit_offset = end_pos as i32 - jump_exit_idx as i32;
        self.chunk.code[jump_exit_idx] = OpCode::Jump {
            offset: JumpOffset(exit_offset),
        };

        Ok(result_reg)
    }

    pub(super) fn compile_match_expr(
        &mut self,
        target: &Expression,
        arms: &[MatchArmExpr],
        _span: Span,
    ) -> Result<Register, EvalError> {
        let target_reg = self.compile_expression(target)?;
        let result_reg = self.alloc_reg();
        let mut exit_jumps = Vec::new();

        for arm in arms {
            let mut next_arm_jump = None;
            match &arm.pattern {
                Pattern::Wildcard { .. } => {}
                Pattern::Expr(pat_expr) => {
                    let pat_reg = self.compile_expression(pat_expr)?;
                    let eq_reg = self.alloc_reg();
                    self.chunk.emit(OpCode::Eq { dst: eq_reg, lhs: target_reg, rhs: pat_reg }, arm.span);
                    let jmp_idx = self.chunk.emit(OpCode::JumpIfFalse { cond: eq_reg, offset: JumpOffset(0) }, arm.span);
                    next_arm_jump = Some(jmp_idx);
                }
            }

            let arm_res = match &arm.body {
                MatchArmBody::Expr(expr) => self.compile_expression(expr)?,
                MatchArmBody::Block(blk) => self.compile_block_expr(blk, arm.span)?,
            };
            self.chunk.emit(OpCode::Move { dst: result_reg, src: arm_res }, arm.span);

            let exit_jmp = self.chunk.emit(OpCode::Jump { offset: JumpOffset(0) }, arm.span);
            exit_jumps.push(exit_jmp);

            if let Some(jmp_idx) = next_arm_jump {
                let cur_pos = self.chunk.code.len();
                let offset = cur_pos as i32 - jmp_idx as i32;
                self.chunk.code[jmp_idx] = OpCode::JumpIfFalse { cond: Register(0), offset: JumpOffset(offset) };
            }
        }

        let end_pos = self.chunk.code.len();
        for jmp_idx in exit_jumps {
            let offset = end_pos as i32 - jmp_idx as i32;
            self.chunk.code[jmp_idx] = OpCode::Jump { offset: JumpOffset(offset) };
        }

        Ok(result_reg)
    }

    pub(super) fn compile_range(
        &mut self,
        start: &Expression,
        end: &Expression,
        inclusive: bool,
        span: Span,
    ) -> Result<Register, EvalError> {
        let s_reg = self.compile_expression(start)?;
        let e_reg = self.compile_expression(end)?;
        let start_args = self.alloc_reg();
        self.chunk.emit(OpCode::Move { dst: start_args, src: s_reg }, span);
        let arg1 = self.alloc_reg();
        self.chunk.emit(OpCode::Move { dst: arg1, src: e_reg }, span);
        let arg2 = self.alloc_reg();
        self.chunk.emit(OpCode::LoadBool { dst: arg2, val: inclusive }, span);

        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::BuiltinCall {
                builtin_id: 0x0D,
                args_start: start_args,
                arg_count: 3,
                dst,
            },
            span,
        );
        Ok(dst)
    }
}
