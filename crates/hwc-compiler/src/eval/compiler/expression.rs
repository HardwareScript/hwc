//! Expression compilation to bytecode

use compact_str::CompactString;
use hwc_parser::ast::*;
use std::sync::Arc;

use crate::eval::builtins;
use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::{MeasurementValue, Value};

use super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    /// Compile an Expression AST node into a result register
    pub fn compile_expression(&mut self, expr: &Expression) -> Result<Register, EvalError> {
        match expr {
            Expression::Literal { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadInt { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::FloatLiteral { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadFloat { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::BooleanLiteral { value, span } => {
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::LoadBool { dst, val: *value }, *span);
                Ok(dst)
            }

            Expression::StringLiteral { value, span } => {
                if let Some((pattern, expr_strs)) = parse_interpolated_string_template(value) {
                    self.compile_interpolated_string(&pattern, &expr_strs, *span)
                } else {
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

    fn compile_interpolated_string(
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

    fn compile_measurement(&mut self, value: f64, unit: &Unit, span: Span) -> Result<Register, EvalError> {
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

    fn compile_variable(&mut self, name: &CompactString, span: Span) -> Result<Register, EvalError> {
        // First check local variable bindings
        if let Some((src_reg, _)) = self.lookup_var(name.as_str()) {
            let dst = self.alloc_reg();
            self.chunk.emit(OpCode::Move { dst, src: src_reg }, span);
            return Ok(dst);
        }
        
        // Then check if it's an enum type (for TapType.P_Sub syntax)
        if let Some(enum_value) = self.enum_types.get(name.as_str()).cloned() {
            let dst = self.alloc_reg();
            let const_idx = self.chunk.add_constant(enum_value);
            self.chunk.emit(OpCode::LoadConst { dst, const_idx }, span);
            return Ok(dst);
        }
        
        // Not found anywhere
        eprintln!("[DEBUG compile_variable] Failed lookup for '{}' in chunk '{}'", name, self.chunk.name);
        Err(EvalError::UndefinedVariable { name: name.clone() })
    }

    fn compile_array_literal(&mut self, elements: &[Expression], span: Span) -> Result<Register, EvalError> {
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
            // Place elements in contiguous registers
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

    fn compile_struct_instance(
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
                // Shorthand { x, y }
                let (src_reg, _) = self.lookup_var(field.name.as_str()).ok_or_else(|| {
                    EvalError::UndefinedVariable { name: field.name.clone() }
                })?;
                src_reg
            };

            // Coerce field value if struct declaration specifies a type (e.g. Point2D)
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

        // Store struct metadata (name + field names) in constant table
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

    fn compile_binary_op(
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

    fn compile_unary_op(
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

    fn compile_range(
        &mut self,
        start: &Expression,
        end: &Expression,
        inclusive: bool,
        span: Span,
    ) -> Result<Register, EvalError> {
        let s_reg = self.compile_expression(start)?;
        let e_reg = self.compile_expression(end)?;
        // Range expression compiles via builtin helper
        let start_args = self.alloc_reg();
        self.chunk.emit(OpCode::Move { dst: start_args, src: s_reg }, span);
        let arg1 = self.alloc_reg();
        self.chunk.emit(OpCode::Move { dst: arg1, src: e_reg }, span);
        let arg2 = self.alloc_reg();
        self.chunk.emit(OpCode::LoadBool { dst: arg2, val: inclusive }, span);

        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::BuiltinCall {
                builtin_id: 0x0D, // Builtin Range
                args_start: start_args,
                arg_count: 3,
                dst,
            },
            span,
        );
        Ok(dst)
    }

    fn compile_field_access(
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

    fn compile_index_access(
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

    fn compile_call(
        &mut self,
        callee: &Expression,
        arguments: &[NamedOrPositionalArg],
        span: Span,
    ) -> Result<Register, EvalError> {
        // 1. Method call on an object / expression: target.method(args)
        if let Expression::FieldAccess { target, field, span: _ } = callee {
            if let Expression::Variable { name, .. } = target.as_ref() {
                if name.as_str() == "space" {
                    return self.compile_space_method_call(field.as_str(), arguments, span);
                }
            }

            match field.as_str() {
                "push" => {
                    if let Some(arg) = arguments.first() {
                        let target_reg = self.compile_expression(target)?;
                        let val_reg = self.compile_expression(&arg.value)?;
                        self.chunk.emit(OpCode::ArrayPush { array_reg: target_reg, val_reg }, span);
                        let dst = self.alloc_reg();
                        self.chunk.emit(OpCode::LoadNull { dst }, span);
                        return Ok(dst);
                    }
                }
                "pop" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::ArrayPop { dst, array_reg: target_reg }, span);
                    return Ok(dst);
                }
                "len" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::ArrayLen { dst, array_reg: target_reg }, span);
                    return Ok(dst);
                }
                "to_float" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::MeasToFloat { dst, src: target_reg }, span);
                    return Ok(dst);
                }
                "to_int" | "to_pm" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::MeasToInt { dst, src: target_reg }, span);
                    return Ok(dst);
                }
                "rotate" => {
                    let target_reg = self.compile_expression(target)?;
                    let deg_reg = if !arguments.is_empty() {
                        self.compile_expression(&arguments[0].value)?
                    } else {
                        return Err(EvalError::General { message: "rotate requires degree argument".into() });
                    };
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellRotate { dst, cell_reg: target_reg, deg_reg }, span);
                    return Ok(dst);
                }
                "mirror_x" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellMirrorX { dst, cell_reg: target_reg }, span);
                    return Ok(dst);
                }
                "mirror_y" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellMirrorY { dst, cell_reg: target_reg }, span);
                    return Ok(dst);
                }
                "offset" => {
                    let target_reg = self.compile_expression(target)?;
                    let (dx_reg, dy_reg) = if arguments.len() >= 2 {
                        (self.compile_expression(&arguments[0].value)?, self.compile_expression(&arguments[1].value)?)
                    } else {
                        return Err(EvalError::General { message: "offset requires dx and dy arguments".into() });
                    };
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellOffset { dst, cell_reg: target_reg, dx_reg, dy_reg }, span);
                    return Ok(dst);
                }
                "port" => {
                    let target_reg = self.compile_expression(target)?;
                    let port_name: CompactString = if !arguments.is_empty() {
                        if let Expression::StringLiteral { value, .. } = &arguments[0].value {
                            value.clone().into()
                        } else if let Expression::Variable { name, .. } = &arguments[0].value {
                            name.clone()
                        } else {
                            "default".into()
                        }
                    } else {
                        "default".into()
                    };
                    let port_idx = self.chunk.add_constant(Value::String(port_name));
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellPort { dst, target_reg, port_name_idx: port_idx }, span);
                    return Ok(dst);
                }
                "bounding_box" => {
                    let target_reg = self.compile_expression(target)?;
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::CellBBox { dst, target_reg }, span);
                    return Ok(dst);
                }
                "add_polygon" => {
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_map = rustc_hash::FxHashMap::default();
                    for arg in arguments {
                        if let Some(name) = &arg.name {
                            let r = self.compile_expression(&arg.value)?;
                            arg_map.insert(name.as_str(), r);
                        }
                    }
                    let layer_reg = if let Some(r) = arg_map.get("layer").copied() {
                        r
                    } else if !arguments.is_empty() && arguments[0].name.is_none() {
                        self.compile_expression(&arguments[0].value)?
                    } else {
                        return Err(EvalError::General { message: "add_polygon requires 'layer'".into() });
                    };

                    let net_reg = arg_map.get("net").copied().unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.chunk.emit(OpCode::LoadNull { dst: r }, span);
                        r
                    });

                    let rect_or_points_reg = if let Some(r) = arg_map.get("rect").copied().or_else(|| arg_map.get("points").copied()) {
                        r
                    } else if arguments.len() > 1 && arguments[1].name.is_none() {
                        self.compile_expression(&arguments[1].value)?
                    } else {
                        return Err(EvalError::General { message: "add_polygon requires 'rect' or 'points'".into() });
                    };

                    self.chunk.emit(OpCode::CellAddPolygon { cell_reg: target_reg, layer_reg, net_reg, rect_or_points_reg }, span);
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst }, span);
                    return Ok(dst);
                }
                "add_contact" => {
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_map = rustc_hash::FxHashMap::default();
                    for arg in arguments {
                        if let Some(name) = &arg.name {
                            let r = self.compile_expression(&arg.value)?;
                            arg_map.insert(name.as_str(), r);
                        }
                    }
                    let from_layer_reg = if let Some(r) = arg_map.get("from").copied() {
                        r
                    } else if !arguments.is_empty() && arguments[0].name.is_none() {
                        self.compile_expression(&arguments[0].value)?
                    } else {
                        return Err(EvalError::General { message: "add_contact requires 'from'".into() });
                    };

                    let to_layer_reg = if let Some(r) = arg_map.get("to").copied() {
                        r
                    } else if arguments.len() > 1 && arguments[1].name.is_none() {
                        self.compile_expression(&arguments[1].value)?
                    } else {
                        return Err(EvalError::General { message: "add_contact requires 'to'".into() });
                    };

                    let at_reg = if let Some(r) = arg_map.get("at").copied() {
                        r
                    } else if arguments.len() > 2 && arguments[2].name.is_none() {
                        self.compile_expression(&arguments[2].value)?
                    } else {
                        return Err(EvalError::General { message: "add_contact requires 'at'".into() });
                    };

                    let dia_reg = if let Some(r) = arg_map.get("diameter").copied() {
                        r
                    } else {
                        let r = self.alloc_reg();
                        let const_idx = self.chunk.add_constant(Value::Int(170_000));
                        self.chunk.emit(OpCode::LoadConst { dst: r, const_idx }, span);
                        r
                    };

                    let net_reg = arg_map.get("net").copied().unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.chunk.emit(OpCode::LoadNull { dst: r }, span);
                        r
                    });

                    self.chunk.emit(OpCode::CellAddContact { cell_reg: target_reg, from_layer_reg, to_layer_reg, at_reg, dia_reg, net_reg }, span);
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst }, span);
                    return Ok(dst);
                }
                "add_port" => {
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_map = rustc_hash::FxHashMap::default();
                    for arg in arguments {
                        if let Some(name) = &arg.name {
                            let r = self.compile_expression(&arg.value)?;
                            arg_map.insert(name.as_str(), r);
                        }
                    }
                    let name_reg = if let Some(r) = arg_map.get("name").copied() {
                        r
                    } else if !arguments.is_empty() && arguments[0].name.is_none() {
                        self.compile_expression(&arguments[0].value)?
                    } else {
                        return Err(EvalError::General { message: "add_port requires 'name'".into() });
                    };

                    let at_reg = if let Some(r) = arg_map.get("at").copied() {
                        r
                    } else if arguments.len() > 1 && arguments[1].name.is_none() {
                        self.compile_expression(&arguments[1].value)?
                    } else {
                        return Err(EvalError::General { message: "add_port requires 'at'".into() });
                    };

                    let layer_reg = if let Some(r) = arg_map.get("layer").copied() {
                        r
                    } else if arguments.len() > 2 && arguments[2].name.is_none() {
                        self.compile_expression(&arguments[2].value)?
                    } else {
                        return Err(EvalError::General { message: "add_port requires 'layer'".into() });
                    };

                    let net_reg = arg_map.get("net").copied().unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.chunk.emit(OpCode::LoadNull { dst: r }, span);
                        r
                    });

                    self.chunk.emit(OpCode::CellAddPort { cell_reg: target_reg, name_reg, at_reg, layer_reg, net_reg }, span);
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst }, span);
                    return Ok(dst);
                }
                "add_device" => {
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_map = rustc_hash::FxHashMap::default();
                    for arg in arguments {
                        if let Some(name) = &arg.name {
                            let r = self.compile_expression(&arg.value)?;
                            arg_map.insert(name.as_str(), r);
                        }
                    }
                    let type_reg = arg_map.get("type").copied().ok_or_else(|| EvalError::General { message: "add_device requires 'type'".into() })?;
                    let terms_reg = arg_map.get("terminals").copied().ok_or_else(|| EvalError::General { message: "add_device requires 'terminals'".into() })?;
                    let params_reg = arg_map.get("params").copied().unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.chunk.emit(OpCode::LoadNull { dst: r }, span);
                        r
                    });

                    self.chunk.emit(OpCode::CellAddDevice { cell_reg: target_reg, type_reg, terms_reg, params_reg }, span);
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst }, span);
                    return Ok(dst);
                }
                "place" => {
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_map = rustc_hash::FxHashMap::default();
                    for arg in arguments {
                        if let Some(name) = &arg.name {
                            let r = self.compile_expression(&arg.value)?;
                            arg_map.insert(name.as_str(), r);
                        }
                    }
                    let child_cell_reg = if let Some(r) = arg_map.get("child").copied().or_else(|| arg_map.get("cell").copied()) {
                        r
                    } else if !arguments.is_empty() && arguments[0].name.is_none() {
                        self.compile_expression(&arguments[0].value)?
                    } else {
                        return Err(EvalError::General { message: "place requires child cell argument".into() });
                    };

                    let at_reg = if let Some(r) = arg_map.get("at").copied() {
                        r
                    } else if arguments.len() > 1 && arguments[1].name.is_none() {
                        self.compile_expression(&arguments[1].value)?
                    } else {
                        return Err(EvalError::General { message: "place requires 'at' coordinate argument".into() });
                    };

                    self.chunk.emit(OpCode::CellPlace { cell_reg: target_reg, child_cell_reg, at_reg }, span);
                    let dst = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst }, span);
                    return Ok(dst);
                }
                _ => {
                    // General struct method dispatch
                    let target_reg = self.compile_expression(target)?;
                    let mut arg_regs = Vec::new();
                    for arg in arguments {
                        arg_regs.push(self.compile_expression(&arg.value)?);
                    }
                    let start_reg = self.alloc_reg();
                    for (i, r) in arg_regs.iter().enumerate() {
                        let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
                        self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
                    }
                    let method_idx = self.chunk.add_constant(Value::String(field.clone()));
                    let dst = self.alloc_reg();
                    self.chunk.emit(
                        OpCode::CallMethod {
                            method_name_idx: method_idx,
                            target_reg,
                            args_start: start_reg,
                            arg_count: arg_regs.len() as u8,
                            dst,
                        },
                        span,
                    );
                    return Ok(dst);
                }
            }
        }

        // 2. Direct CellLayout::new constructor call
        if let Expression::Path { segments, .. } = callee {
            if segments.len() == 2 && segments[0] == "CellLayout" && segments[1] == "new" {
                let name_reg = if let Some(arg) = arguments.iter().find(|a| a.name.as_deref() == Some("name")) {
                    self.compile_expression(&arg.value)?
                } else if !arguments.is_empty() {
                    self.compile_expression(&arguments[0].value)?
                } else {
                    let const_idx = self.chunk.add_constant(Value::String("cell".into()));
                    let r = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadConst { dst: r, const_idx }, span);
                    r
                };
                let dst = self.alloc_reg();
                self.chunk.emit(OpCode::CellNew { dst, name_reg }, span);
                return Ok(dst);
            }
        }

        // 3. Builtin function call (println, assert, min, max, abs, sqrt, rect_between, int, float, bbox_*)
        if let Expression::Variable { name, .. } = callee {
            if let Some(builtin_id) = builtins::get_builtin_id(name.as_str()) {
                return self.compile_builtin_call(builtin_id, arguments, span);
            }
        }

        // 3. User function call
        self.compile_user_function_call(callee, arguments, span)
    }

    fn compile_tuple(&mut self, elements: &[Expression], span: Span) -> Result<Register, EvalError> {
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

    fn compile_slice(
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

    fn compile_block_expr(&mut self, block: &Block, span: Span) -> Result<Register, EvalError> {
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

    fn compile_if_expr(
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
                offset: crate::eval::opcodes::JumpOffset(0),
            },
            span,
        );

        let result_reg = self.alloc_reg();

        // Compile then branch
        let then_res = self.compile_block_expr(then_branch, span)?;
        self.chunk.emit(OpCode::Move { dst: result_reg, src: then_res }, span);

        let jump_exit_idx = self.chunk.emit(
            OpCode::Jump {
                offset: crate::eval::opcodes::JumpOffset(0),
            },
            span,
        );

        // Patch jump_false
        let else_start = self.chunk.code.len();
        let offset = else_start as i32 - jump_false_idx as i32;
        self.chunk.code[jump_false_idx] = OpCode::JumpIfFalse {
            cond: cond_reg,
            offset: crate::eval::opcodes::JumpOffset(offset),
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

        // Patch exit jump
        let end_pos = self.chunk.code.len();
        let exit_offset = end_pos as i32 - jump_exit_idx as i32;
        self.chunk.code[jump_exit_idx] = OpCode::Jump {
            offset: crate::eval::opcodes::JumpOffset(exit_offset),
        };

        Ok(result_reg)
    }

    fn compile_match_expr(
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
                Pattern::Wildcard { .. } => {
                    // Always matches
                }
                Pattern::Expr(pat_expr) => {
                    let pat_reg = self.compile_expression(pat_expr)?;
                    let eq_reg = self.alloc_reg();
                    self.chunk.emit(OpCode::Eq { dst: eq_reg, lhs: target_reg, rhs: pat_reg }, arm.span);
                    let jmp_idx = self.chunk.emit(OpCode::JumpIfFalse { cond: eq_reg, offset: crate::eval::opcodes::JumpOffset(0) }, arm.span);
                    next_arm_jump = Some(jmp_idx);
                }
            }

            let arm_res = match &arm.body {
                MatchArmBody::Expr(expr) => self.compile_expression(expr)?,
                MatchArmBody::Block(blk) => self.compile_block_expr(blk, arm.span)?,
            };
            self.chunk.emit(OpCode::Move { dst: result_reg, src: arm_res }, arm.span);

            let exit_jmp = self.chunk.emit(OpCode::Jump { offset: crate::eval::opcodes::JumpOffset(0) }, arm.span);
            exit_jumps.push(exit_jmp);

            if let Some(jmp_idx) = next_arm_jump {
                let cur_pos = self.chunk.code.len();
                let offset = cur_pos as i32 - jmp_idx as i32;
                self.chunk.code[jmp_idx] = OpCode::JumpIfFalse { cond: Register(0), offset: crate::eval::opcodes::JumpOffset(offset) };
            }
        }

        let end_pos = self.chunk.code.len();
        for jmp_idx in exit_jumps {
            let offset = end_pos as i32 - jmp_idx as i32;
            self.chunk.code[jmp_idx] = OpCode::Jump { offset: crate::eval::opcodes::JumpOffset(offset) };
        }

        Ok(result_reg)
    }

    fn compile_builtin_call(
        &mut self,
        builtin_id: u8,
        arguments: &[NamedOrPositionalArg],
        span: Span,
    ) -> Result<Register, EvalError> {
        let mut arg_regs = Vec::new();
        for arg in arguments {
            arg_regs.push(self.compile_expression(&arg.value)?);
        }
        let start_reg = self.alloc_reg();
        for (i, r) in arg_regs.iter().enumerate() {
            let target_r = if i == 0 { start_reg } else { self.alloc_reg() };
            self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
        }
        let dst = self.alloc_reg();
        self.chunk.emit(
            OpCode::BuiltinCall {
                builtin_id,
                args_start: start_reg,
                arg_count: arg_regs.len() as u8,
                dst,
            },
            span,
        );
        Ok(dst)
    }

    fn compile_user_function_call(
        &mut self,
        callee: &Expression,
        arguments: &[NamedOrPositionalArg],
        span: Span,
    ) -> Result<Register, EvalError> {
        let fn_name = match callee {
            Expression::Variable { name, .. } => name.clone(),
            Expression::Path { segments, .. } => segments.join("::").into(),
            _ => {
                return Err(EvalError::General {
                    message: "Call target must be an identifier or path".into(),
                })
            }
        };

        let func_decl = self.function_decls.get(&fn_name).cloned();
        let param_names: Vec<CompactString> = func_decl
            .as_ref()
            .map(|f| f.parameters.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();

        // Order arguments to match function parameters (supporting named arguments)
        let mut ordered_arg_exprs: Vec<Option<&Expression>> = vec![None; param_names.len().max(arguments.len())];
        for (i, arg) in arguments.iter().enumerate() {
            if let Some(arg_name) = &arg.name {
                if let Some(pos) = param_names.iter().position(|p| p == arg_name) {
                    ordered_arg_exprs[pos] = Some(&arg.value);
                } else {
                    // Extra named arg
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
                        message: format!("Cannot evaluate argument {} for function {}", i, fn_name),
                    });
                }
            } else {
                return Err(EvalError::General {
                    message: format!("Cannot evaluate argument {} for function {}", i, fn_name),
                });
            };

            let final_reg = if let Some(f) = &func_decl {
                if i < f.parameters.len() {
                    if let TypeExpr::Named { name: type_name, .. } = &f.parameters[i].type_annotation {
                        if type_name.as_str() == "Point2D" {
                            let coerced_reg = self.alloc_reg();
                            self.chunk.emit(
                                OpCode::CoercePoint2D {
                                    dst: coerced_reg,
                                    src: raw_reg,
                                },
                                span,
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
            self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
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
            span,
        );

        Ok(dst)
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
