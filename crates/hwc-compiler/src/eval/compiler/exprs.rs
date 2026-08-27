//! `BytecodeCompiler::compile_expression` — compiles every expression variant to bytecode.
//!
//! Expression variants handled here:
//!   `Literal`, `FloatLiteral`, `BooleanLiteral`, `StringLiteral`, `Measurement`,
//!   `Variable`, `ArrayLiteral`, `StructInstance`, `Binary`, `Unary`, `Range`,
//!   `FieldAccess`, `Index`, `Grouped`, `Call`
//!
//! Space method calls (`space.add_polygon`, `space.add_contact`, `space.add_device`)
//! are delegated to `space_methods.rs`.

use compact_str::CompactString;
use hwc_parser::ast::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::super::builtins;
use super::super::context::EvalError;
use super::super::opcodes::{OpCode, Register};
use super::super::value::{MeasurementValue, Value};
use super::core::BytecodeCompiler;
use super::string_interp::parse_interpolated_string_template;

impl<'a> BytecodeCompiler<'a> {
    /// Compile an `Expression` AST node, emitting opcodes and returning the result register.
    pub fn compile_expression(&mut self, expr: &Expression) -> Result<Register, EvalError> {
        match expr {
            // ── Integer literal ───────────────────────────────────────────────
            Expression::Literal { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadInt { dst, val: *value }, *span);
                Ok(dst)
            }

            // ── Float literal ─────────────────────────────────────────────────
            Expression::FloatLiteral { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadFloat { dst, val: *value }, *span);
                Ok(dst)
            }

            // ── Boolean literal ───────────────────────────────────────────────
            Expression::BooleanLiteral { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadBool { dst, val: *value }, *span);
                Ok(dst)
            }

            // ── String literal (possibly interpolated) ────────────────────────
            Expression::StringLiteral { value, span } => {
                if let Some((pattern, expr_strs)) = parse_interpolated_string_template(value) {
                    let mut arg_regs = Vec::new();
                    for expr_str in expr_strs {
                        let lexer = hwc_parser::Lexer::new(&expr_str);
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
                            self.chunk.emit(OpCode::Move { dst, src: src_reg }, *span);
                            arg_regs.push(dst);
                        } else {
                            let dst = self.alloc_reg();
                            let const_idx =
                                self.chunk.add_constant(Value::String(expr_str.into()));
                            self.chunk.emit(OpCode::LoadConst { dst, const_idx }, *span);
                            arg_regs.push(dst);
                        }
                    }

                    let start_reg = self.alloc_reg();
                    if let Some(&first) = arg_regs.first() {
                        self.chunk.emit(OpCode::Move { dst: start_reg, src: first }, *span);
                        for &r in &arg_regs[1..] {
                            let next = self.alloc_reg();
                            self.chunk.emit(OpCode::Move { dst: next, src: r }, *span);
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
                        *span,
                    );
                    Ok(dst)
                } else {
                    let dst = self.alloc_reg();
                    let const_idx =
                        self.chunk.add_constant(Value::String(value.clone().into()));
                    self.chunk.emit(OpCode::LoadConst { dst, const_idx }, *span);
                    Ok(dst)
                }
            }

            // ── Physical measurement literal (e.g. 10kΩ) ─────────────────────
            Expression::Measurement { value, unit, span } => {
                let dst = self.alloc_reg();
                let val =
                    if let Some(m) = MeasurementValue::from_ast_unit(*value, unit, self.unit_registry)
                    {
                        Value::Measurement(m)
                    } else {
                        Value::Float(*value)
                    };
                let const_idx = self.chunk.add_constant(val);
                self.chunk.emit(OpCode::LoadConst { dst, const_idx }, *span);
                Ok(dst)
            }

            // ── Variable reference ────────────────────────────────────────────
            Expression::Variable { name, span } => {
                // Check local variable bindings first
                if let Some((src_reg, _)) = self.lookup_var(name.as_str()) {
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::Move { dst, src: src_reg }, *span);
                    return Ok(dst);
                }

                // Then check enum types (e.g. `TapType` used without `.` field access)
                if let Some(enum_value) = self.enum_types.get(name.as_str()).cloned() {
                    let dst = self.alloc_reg();
                    let const_idx = self.chunk.add_constant(enum_value);
                    self.chunk.emit(OpCode::LoadConst { dst, const_idx }, *span);
                    return Ok(dst);
                }

                Err(EvalError::UndefinedVariable { name: name.clone() })
            }

            // ── Array literal ─────────────────────────────────────────────────
            Expression::ArrayLiteral { elements, span } => {
                let mut elem_regs = Vec::new();
                for elem in elements {
                    elem_regs.push(self.compile_expression(elem)?);
                }

                let dst = self.alloc_reg();
                if elem_regs.is_empty() {
                    let start_reg = self.alloc_reg();
                    self.chunk.emit(
                        OpCode::AllocArray { dst, start_reg, count: 0 },
                        *span,
                    );
                } else {
                    // Place elements in contiguous registers
                    let start_reg = self.alloc_reg();
                    for (i, reg) in elem_regs.iter().enumerate() {
                        let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
                        self.chunk.emit(OpCode::Move { dst: target_r, src: *reg }, *span);
                    }
                    self.chunk.emit(
                        OpCode::AllocArray {
                            dst,
                            start_reg,
                            count: elem_regs.len() as u16,
                        },
                        *span,
                    );
                }
                Ok(dst)
            }

            // ── Struct instantiation ──────────────────────────────────────────
            Expression::StructInstance { name, fields, span } => {
                let struct_decl = self.struct_decls.get(name.as_str()).cloned();
                eprintln!(
                    "[BYTECODE DEBUG] Compiling StructInstance '{}', struct_decl found: {}",
                    name,
                    struct_decl.is_some()
                );
                let mut field_names = Vec::new();
                let mut field_regs = Vec::new();

                for field in fields {
                    field_names.push(field.name.clone());
                    let raw_val_r = if let Some(v_expr) = &field.value {
                        self.compile_expression(v_expr)?
                    } else {
                        // Shorthand { x, y }
                        let (src_reg, _) =
                            self.lookup_var(field.name.as_str()).ok_or_else(|| {
                                EvalError::UndefinedVariable { name: field.name.clone() }
                            })?;
                        src_reg
                    };

                    // Coerce field value if the struct declaration specifies a type
                    let val_r = if let Some(decl) = &struct_decl {
                        if let Some(decl_field) =
                            decl.fields.iter().find(|f| f.name == field.name)
                        {
                            if let TypeExpr::Named { name: type_name, .. } =
                                &decl_field.type_annotation
                            {
                                if type_name.as_str() == "Point2D" {
                                    eprintln!(
                                        "[BYTECODE DEBUG] Coercing field '{}.{}' to Point2D",
                                        name, field.name
                                    );
                                    let coerced_reg = self.alloc_reg();
                                    self.chunk.emit(
                                        OpCode::CoercePoint2D {
                                            dst: coerced_reg,
                                            src: raw_val_r,
                                        },
                                        *span,
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
                    self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, *span);
                }

                // Store struct metadata (name + field names) in the constant table
                let struct_meta = Value::StructInstance {
                    name: name.clone(),
                    fields: Arc::new(
                        field_names.into_iter().map(|n| (n, Value::Void)).collect(),
                    ),
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
                    *span,
                );
                Ok(dst)
            }

            // ── Binary operation ──────────────────────────────────────────────
            Expression::Binary { left, operator, right, span } => {
                let lhs = self.compile_expression(left)?;
                let rhs = self.compile_expression(right)?;
                let dst = self.alloc_reg();

                match operator {
                    BinaryOperator::Add => self.chunk.emit(OpCode::Add { dst, lhs, rhs }, *span),
                    BinaryOperator::Subtract => {
                        self.chunk.emit(OpCode::Sub { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::Multiply => {
                        self.chunk.emit(OpCode::Mul { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::Divide => {
                        self.chunk.emit(OpCode::Div { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::Modulo => {
                        self.chunk.emit(OpCode::Mod { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::Equal => self.chunk.emit(OpCode::Eq { dst, lhs, rhs }, *span),
                    BinaryOperator::NotEqual => {
                        self.chunk.emit(OpCode::Ne { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::LessThan => {
                        self.chunk.emit(OpCode::Lt { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::LessThanOrEqual => {
                        self.chunk.emit(OpCode::Le { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::GreaterThan => {
                        self.chunk.emit(OpCode::Gt { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::GreaterThanOrEqual => {
                        self.chunk.emit(OpCode::Ge { dst, lhs, rhs }, *span)
                    }
                    BinaryOperator::And => self.chunk.emit(OpCode::And { dst, lhs, rhs }, *span),
                    BinaryOperator::Or => self.chunk.emit(OpCode::Or { dst, lhs, rhs }, *span),
                };

                Ok(dst)
            }

            // ── Unary operation ───────────────────────────────────────────────
            Expression::Unary { operator, operand, span } => {
                let src = self.compile_expression(operand)?;
                let dst = self.alloc_reg();
                match operator {
                    UnaryOperator::Not => self.chunk.emit(OpCode::Not { dst, src }, *span),
                    UnaryOperator::Negate => self.chunk.emit(OpCode::Neg { dst, src }, *span),
                    UnaryOperator::Plus => self.chunk.emit(OpCode::Move { dst, src }, *span),
                };
                Ok(dst)
            }

            // ── Range expression (start..end or start..=end) ──────────────────
            Expression::Range { start, end, inclusive, span } => {
                let s_reg = self.compile_expression(start)?;
                let e_reg = self.compile_expression(end)?;
                let start_args = self.alloc_reg();
                self.chunk.emit(OpCode::Move { dst: start_args, src: s_reg }, *span);
                let arg1 = self.alloc_reg();
                self.chunk.emit(OpCode::Move { dst: arg1, src: e_reg }, *span);
                let arg2 = self.alloc_reg();
                self.chunk.emit(OpCode::LoadBool { dst: arg2, val: *inclusive }, *span);

                let dst = self.alloc_reg();
                self.chunk.emit(
                    OpCode::BuiltinCall {
                        builtin_id: 0x0D, // Builtin Range
                        args_start: start_args,
                        arg_count: 3,
                        dst,
                    },
                    *span,
                );
                Ok(dst)
            }

            // ── Field access (obj.field) ──────────────────────────────────────
            Expression::FieldAccess { target, field, span } => {
                let obj = self.compile_expression(target)?;
                let field_idx = self.chunk.add_constant(Value::String(field.clone()));
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::GetField { dst, obj, field_idx }, *span);
                Ok(dst)
            }

            // ── Index access (obj[index]) ─────────────────────────────────────
            Expression::Index { target, index, span } => {
                let obj = self.compile_expression(target)?;
                let idx_reg = self.compile_expression(index)?;
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::GetIndex { dst, obj, index: idx_reg }, *span);
                Ok(dst)
            }

            // ── Grouped expression (parenthesised) ────────────────────────────
            Expression::Grouped { expression, .. } => self.compile_expression(expression),

            // ── Function / method call ────────────────────────────────────────
            Expression::Call { callee, arguments, span } => {
                // 1. Method call on `space.*`
                if let Expression::FieldAccess { target, field, .. } = callee.as_ref() {
                    if let Expression::Variable { name, .. } = target.as_ref() {
                        if name.as_str() == "space" {
                            return self.compile_space_method_call(
                                field.as_str(),
                                arguments,
                                *span,
                            );
                        }
                    }
                }

                // 2. Built-in function call (println, assert, min, max, abs, sqrt, …)
                if let Expression::Variable { name, .. } = callee.as_ref() {
                    if let Some(builtin_id) = builtins::get_builtin_id(name.as_str()) {
                        let mut arg_regs = Vec::new();
                        for arg in arguments {
                            arg_regs.push(self.compile_expression(&arg.value)?);
                        }
                        let start_reg = self.alloc_reg();
                        for (i, r) in arg_regs.iter().enumerate() {
                            let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
                            self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, *span);
                        }
                        let dst = self.alloc_reg();
                        self.chunk.emit(
                            OpCode::BuiltinCall {
                                builtin_id,
                                args_start: start_reg,
                                arg_count: arg_regs.len() as u8,
                                dst,
                            },
                            *span,
                        );
                        return Ok(dst);
                    }
                }

                // 3. User-defined function call
                let fn_name = match callee.as_ref() {
                    Expression::Variable { name, .. } => name.clone(),
                    _ => {
                        return Err(EvalError::General {
                            message: "Call target must be an identifier".into(),
                        })
                    }
                };

                let func_decl = self.function_decls.get(&fn_name).cloned();
                let param_names: Vec<CompactString> = func_decl
                    .as_ref()
                    .map(|f| f.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();

                // Order arguments to match function parameters (supports named arguments)
                let mut ordered_arg_exprs: Vec<Option<&Expression>> =
                    vec![None; param_names.len().max(arguments.len())];
                for (i, arg) in arguments.iter().enumerate() {
                    if let Some(arg_name) = &arg.name {
                        if let Some(pos) = param_names.iter().position(|p| p == arg_name) {
                            ordered_arg_exprs[pos] = Some(&arg.value);
                        } else {
                            // Extra named arg not matching a parameter name
                            ordered_arg_exprs.push(Some(&arg.value));
                        }
                    } else if i < ordered_arg_exprs.len() {
                        ordered_arg_exprs[i] = Some(&arg.value);
                    }
                }

                let mut arg_regs = Vec::new();
                for (i, maybe_expr) in ordered_arg_exprs.iter().enumerate() {
                    let raw_reg = if let Some(expr) = maybe_expr {
                        self.compile_expression(expr)?
                    } else if let Some(f) = &func_decl {
                        if i < f.parameters.len() {
                            if let Some(def_expr) = &f.parameters[i].default_value {
                                self.compile_expression(def_expr)?
                            } else {
                                return Err(EvalError::MissingArgument {
                                    param: f.parameters[i].name.clone(),
                                    func: fn_name,
                                });
                            }
                        } else {
                            return Err(EvalError::General {
                                message: format!(
                                    "Cannot evaluate argument {} for function {}",
                                    i, fn_name
                                ),
                            });
                        }
                    } else {
                        return Err(EvalError::General {
                            message: format!(
                                "Cannot evaluate argument {} for function {}",
                                i, fn_name
                            ),
                        });
                    };

                    // Coerce Point2D arguments at call-site
                    let final_reg = if let Some(f) = &func_decl {
                        if i < f.parameters.len() {
                            if let TypeExpr::Named { name: type_name, .. } =
                                &f.parameters[i].type_annotation
                            {
                                if type_name.as_str() == "Point2D" {
                                    let coerced_reg = self.alloc_reg();
                                    self.chunk.emit(
                                        OpCode::CoercePoint2D { dst: coerced_reg, src: raw_reg },
                                        *span,
                                    );
                                    coerced_reg
                                } else {
                                    raw_reg
                                }
                            } else {
                                raw_reg
                            }
                        } else {
                            raw_reg
                        }
                    } else {
                        raw_reg
                    };

                    arg_regs.push(final_reg);
                }

                let start_reg = self.alloc_reg();
                for (i, r) in arg_regs.iter().enumerate() {
                    let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
                    self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, *span);
                }

                let fn_name_const = self.chunk.add_constant(Value::String(fn_name));
                let dst = self.alloc_reg();
                self.chunk.emit(
                    OpCode::Call {
                        func_name_idx: fn_name_const,
                        args_start: start_reg,
                        arg_count: arg_regs.len() as u8,
                        dst,
                    },
                    *span,
                );

                Ok(dst)
            }
        }
    }
}
