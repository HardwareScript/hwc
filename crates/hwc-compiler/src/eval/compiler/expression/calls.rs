use compact_str::CompactString;
use hwc_parser::ast::{Expression, NamedOrPositionalArg, Span};

use crate::eval::builtins;
use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};
use crate::eval::value::Value;

use super::super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    pub(super) fn compile_call(
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

                    let port_reg = arg_map.get("port").copied().unwrap_or_else(|| {
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

                    self.chunk.emit(OpCode::CellAddPolygon { cell_reg: target_reg, layer_reg, net_reg, rect_or_points_reg, port_reg }, span);
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

        // 3. Builtin function call
        if let Expression::Variable { name, .. } = callee {
            if let Some(builtin_id) = builtins::get_builtin_id(name.as_str()) {
                return self.compile_builtin_call(builtin_id, arguments, span);
            }
        }

        // 4. User function call
        self.compile_user_function_call(callee, arguments, span)
    }

    pub(super) fn compile_builtin_call(
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

    pub(super) fn compile_user_function_call(
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

        let mut ordered_arg_exprs: Vec<Option<&Expression>> = vec![None; param_names.len().max(arguments.len())];
        for (i, arg) in arguments.iter().enumerate() {
            if let Some(arg_name) = &arg.name {
                if let Some(pos) = param_names.iter().position(|p| p == arg_name) {
                    ordered_arg_exprs[pos] = Some(&arg.value);
                } else {
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
                    if let hwc_parser::ast::TypeExpr::Named { name: type_name, .. } = &f.parameters[i].type_annotation {
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
