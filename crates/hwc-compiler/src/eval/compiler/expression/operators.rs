use hwc_parser::ast::{BinaryOperator, Expression, Span, UnaryOperator};

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_binary_op(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        span: Span,
    ) -> Result<Register, EvalError> {
        let lhs = self.compile_expression(left)?;
        let rhs = self.compile_expression(right)?;
        let dst = self.alloc_reg();

        match operator {
            BinaryOperator::Add => self.chunk.emit(OpCode::Add { dst, lhs, rhs }, span),
            BinaryOperator::Subtract => self.chunk.emit(OpCode::Sub { dst, lhs, rhs }, span),
            BinaryOperator::Multiply => self.chunk.emit(OpCode::Mul { dst, lhs, rhs }, span),
            BinaryOperator::Divide => self.chunk.emit(OpCode::Div { dst, lhs, rhs }, span),
            BinaryOperator::Modulo => self.chunk.emit(OpCode::Mod { dst, lhs, rhs }, span),
            BinaryOperator::Equal => self.chunk.emit(OpCode::Eq { dst, lhs, rhs }, span),
            BinaryOperator::NotEqual => self.chunk.emit(OpCode::Ne { dst, lhs, rhs }, span),
            BinaryOperator::LessThan => self.chunk.emit(OpCode::Lt { dst, lhs, rhs }, span),
            BinaryOperator::LessThanOrEqual => self.chunk.emit(OpCode::Le { dst, lhs, rhs }, span),
            BinaryOperator::GreaterThan => self.chunk.emit(OpCode::Gt { dst, lhs, rhs }, span),
            BinaryOperator::GreaterThanOrEqual => self.chunk.emit(OpCode::Ge { dst, lhs, rhs }, span),
            BinaryOperator::And => self.chunk.emit(OpCode::And { dst, lhs, rhs }, span),
            BinaryOperator::Or => self.chunk.emit(OpCode::Or { dst, lhs, rhs }, span),
            BinaryOperator::BitwiseAnd => self.chunk.emit(OpCode::BitwiseAnd { dst, lhs, rhs }, span),
            BinaryOperator::BitwiseOr => self.chunk.emit(OpCode::BitwiseOr { dst, lhs, rhs }, span),
            BinaryOperator::BitwiseXor => self.chunk.emit(OpCode::BitwiseXor { dst, lhs, rhs }, span),
            BinaryOperator::ShiftLeft => self.chunk.emit(OpCode::ShiftLeft { dst, lhs, rhs }, span),
            BinaryOperator::ShiftRight => self.chunk.emit(OpCode::ShiftRight { dst, lhs, rhs }, span),
        };

        Ok(dst)
    }

    pub(super) fn compile_unary_op(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        span: Span,
    ) -> Result<Register, EvalError> {
        let src = self.compile_expression(operand)?;
        let dst = self.alloc_reg();
        match operator {
            UnaryOperator::Not => self.chunk.emit(OpCode::Not { dst, src }, span),
            UnaryOperator::Negate => self.chunk.emit(OpCode::Neg { dst, src }, span),
            UnaryOperator::Plus => self.chunk.emit(OpCode::Move { dst, src }, span),
            UnaryOperator::BitwiseNot => self.chunk.emit(OpCode::BitwiseNot { dst, src }, span),
        };
        Ok(dst)
    }
}
