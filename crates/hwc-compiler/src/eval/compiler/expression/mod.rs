//! Expression compilation to bytecode

mod access;
mod calls;
mod collections;
mod control_flow;
mod literals;
mod measurements;
mod operators;
mod structs;
mod variables;

use compact_str::CompactString;
use hwc_parser::ast::Expression;

use crate::eval::context::EvalError;
use crate::eval::opcodes::Register;

use super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    /// Compile an Expression AST node into a result register
    pub fn compile_expression(&mut self, expr: &Expression) -> Result<Register, EvalError> {
        match expr {
            Expression::Literal { value, span } => {
                use crate::eval::opcodes::OpCode;
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadInt { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::FloatLiteral { value, span } => {
                use crate::eval::opcodes::OpCode;
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadFloat { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::BooleanLiteral { value, span } => {
                use crate::eval::opcodes::OpCode;
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadBool { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::StringLiteral { value, span } => {
                if let Some((pattern, expr_strs)) = parse_interpolated_string_template(value) {
                    self.compile_interpolated_string(&pattern, &expr_strs, *span)
                } else {
                use crate::eval::opcodes::OpCode;
                use crate::eval::value::Value;
                let dst = self.alloc_reg();
                    let const_idx = self.chunk.add_constant(Value::String(value.clone().into()));
                    self.chunk.emit(OpCode::LoadConst { dst, const_idx }, *span);
                    Ok(dst)
                }
            }

            Expression::Measurement { value, unit, span } => {
                self.compile_measurement(*value, unit, *span)
            }

            Expression::Variable { name, span } => {
                self.compile_variable(name, *span)
            }

            Expression::ArrayLiteral { elements, span } => {
                self.compile_array_literal(elements, *span)
            }

            Expression::StructInstance { name, fields, span } => {
                self.compile_struct_instance(name, fields, *span)
            }

            Expression::Binary {
                left,
                operator,
                right,
                span,
            } => {
                self.compile_binary_op(left, *operator, right, *span)
            }

            Expression::Unary {
                operator,
                operand,
                span,
            } => {
                self.compile_unary_op(*operator, operand, *span)
            }

            Expression::Range {
                start,
                end,
                inclusive,
                span,
            } => {
                self.compile_range(start, end, *inclusive, *span)
            }

            Expression::FieldAccess { target, field, span } => {
                self.compile_field_access(target, field, *span)
            }

            Expression::Index { target, index, span } => {
                self.compile_index_access(target, index, *span)
            }

            Expression::Tuple { elements, span } => {
                self.compile_tuple(elements, *span)
            }

            Expression::Slice { target, start, end, inclusive, span } => {
                self.compile_slice(target, start.as_deref(), end.as_deref(), *inclusive, *span)
            }

            Expression::If { condition, then_branch, else_branch, span } => {
                self.compile_if_expr(condition, then_branch, else_branch.as_deref(), *span)
            }

            Expression::Match { target, arms, span } => {
                self.compile_match_expr(target, arms, *span)
            }

            Expression::Block { block, span } => {
                self.compile_block_expr(block, *span)
            }

            Expression::Grouped { expression, .. } => self.compile_expression(expression),

            Expression::Path { segments, span } => {
                let path_name: CompactString = segments.join("::").into();
                self.compile_variable(&path_name, *span)
            }

            Expression::Call {
                callee,
                arguments,
                span,
            } => {
                self.compile_call(callee, arguments, *span)
            }
        }
    }
}

/// Helper to parse string template into pattern with `{0}`, `{1}` placeholders and list of expression strings
fn parse_interpolated_string_template(s: &str) -> Option<(String, Vec<String>)> {
    if !s.contains('{') || !s.contains('}') {
        return None;
    }
    let mut pattern = String::new();
    let mut expressions = Vec::new();
    let mut chars = s.char_indices().peekable();
    let mut last_idx = 0;

    while let Some((i, c)) = chars.next() {
        if c == '{' {
            if chars.peek().map(|&(_, next_c)| next_c) == Some('{') {
                // Escaped {{
                pattern.push_str(&s[last_idx..i]);
                pattern.push('{');
                chars.next();
                last_idx = i + 2;
                continue;
            }

            // Find matching }
            let expr_start = i + 1;
            let mut depth = 1;
            let mut expr_end = None;
            while let Some((j, inner_c)) = chars.next() {
                if inner_c == '{' {
                    depth += 1;
                } else if inner_c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        expr_end = Some(j);
                        break;
                    }
                }
            }

            if let Some(end_idx) = expr_end {
                pattern.push_str(&s[last_idx..i]);
                let placeholder = format!("{{{}}}", expressions.len());
                pattern.push_str(&placeholder);
                let expr_str = s[expr_start..end_idx].trim().to_string();
                expressions.push(expr_str);
                last_idx = end_idx + 1;
            } else {
                return None;
            }
        }
    }

    if !expressions.is_empty() {
        if last_idx < s.len() {
            pattern.push_str(&s[last_idx..]);
        }
        Some((pattern, expressions))
    } else {
        None
    }
}
