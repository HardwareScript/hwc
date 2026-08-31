//! Statement compilation to bytecode

use compact_str::CompactString;
use hwc_parser::ast::*;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{JumpOffset, OpCode, Register};
use crate::eval::value::Value;

use super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    /// Compile a single Statement
    pub fn compile_statement(&mut self, stmt: &Statement) -> Result<(), EvalError> {
        match stmt {
            Statement::Let {
                mutable,
                pattern,
                type_annotation,
                value,
                span,
            } => {
                self.compile_let_statement(*mutable, pattern, type_annotation, value, *span)
            }

            Statement::Assignment {
                target,
                operator,
                value,
                span,
            } => {
                self.compile_assignment(target, *operator, value, *span)
            }

            Statement::If {
                condition,
                then_block,
                else_branch,
                span,
            } => {
                self.compile_if_statement(condition, then_block, else_branch, *span)
            }

            Statement::For {
                variables,
                iterable,
                body,
                span,
                ..
            } => {
                self.compile_for_loop(variables, iterable, body, *span)
            }

            Statement::Break { span } => {
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    let jmp_idx = self.chunk.emit(OpCode::Jump { offset: JumpOffset(0) }, *span);
                    loop_ctx.break_jumps.push(jmp_idx);
                    Ok(())
                } else {
                    Err(EvalError::General { message: "break outside of loop".into() })
                }
            }

            Statement::Continue { span } => {
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    let jmp_idx = self.chunk.emit(OpCode::Jump { offset: JumpOffset(0) }, *span);
                    loop_ctx.continue_jumps.push(jmp_idx);
                    Ok(())
                } else {
                    Err(EvalError::General { message: "continue outside of loop".into() })
                }
            }

            Statement::Match { target, arms, span: _span } => {
                self.compile_match_statement(target, arms)
            }

            Statement::Return { value, span } => {
                let ret_reg = if let Some(v) = value {
                    self.compile_expression(v)?
                } else {
                    let r = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadNull { dst: r }, *span);
                    r
                };
                self.chunk.emit(OpCode::Return { val: ret_reg }, *span);
                Ok(())
            }

            Statement::Assert {
                condition,
                message,
                span,
                ..
            } => {
                let cond_reg = self.compile_expression(condition)?;
                let msg = message.as_deref().unwrap_or("Assertion failed");
                let msg_const = self.chunk.add_constant(Value::String(msg.into()));
                self.chunk.emit(
                    OpCode::Assert {
                        cond: cond_reg,
                        msg_idx: msg_const,
                    },
                    *span,
                );
                Ok(())
            }

            Statement::Expression { expression, .. } => {
                self.compile_expression(expression)?;
                Ok(())
            }

            Statement::Route {
                from,
                to,
                intent,
                body,
                span,
            } => {
                self.compile_route_statement(from, to, intent, body, *span)
            }

            Statement::Logic(_) | Statement::Reg(_) | Statement::Region(_) => {
                // Handled downstream during synthesis (Phase 3) or physical compilation
                Ok(())
            }
        }
    }

    fn compile_let_statement(
        &mut self,
        mutable: bool,
        pattern: &BindingPattern,
        type_annotation: &Option<TypeExpr>,
        value: &Expression,
        span: Span,
    ) -> Result<(), EvalError> {
        let val_reg = self.compile_expression(value)?;

        match pattern {
            BindingPattern::Identifier(name) => {
                let final_reg = if let Some(type_expr) = type_annotation {
                    if let TypeExpr::Named { name: type_name, .. } = type_expr {
                        let coerced_reg = self.alloc_reg();
                        if type_name.as_str() == "Point2D" {
                            self.chunk.emit(
                                OpCode::CoercePoint2D {
                                    dst: coerced_reg,
                                    src: val_reg,
                                },
                                span,
                            );
                        } else {
                            let type_const = self.chunk.add_constant(Value::String(type_name.clone()));
                            self.chunk.emit(
                                OpCode::CoerceType {
                                    dst: coerced_reg,
                                    src: val_reg,
                                    type_name_idx: type_const,
                                },
                                span,
                            );
                        }
                        coerced_reg
                    } else {
                        val_reg
                    }
                } else {
                    val_reg
                };

                let dest_reg = self.alloc_reg();
                self.chunk.emit(
                    OpCode::Move {
                        dst: dest_reg,
                        src: final_reg,
                    },
                    span,
                );
                self.bind_var(name.clone(), dest_reg, mutable);
            }
            BindingPattern::Tuple(names) => {
                for (i, name) in names.iter().enumerate() {
                    let idx_reg = self.alloc_reg();
                    self.chunk.emit(OpCode::LoadInt { dst: idx_reg, val: i as i64 }, span);
                    let elem_reg = self.alloc_reg();
                    self.chunk.emit(
                        OpCode::GetIndex {
                            dst: elem_reg,
                            obj: val_reg,
                            index: idx_reg,
                        },
                        span,
                    );
                    self.bind_var(name.clone(), elem_reg, mutable);
                }
            }
        }
        Ok(())
    }

    fn compile_assignment(
        &mut self,
        target: &Expression,
        operator: AssignmentOperator,
        value: &Expression,
        span: Span,
    ) -> Result<(), EvalError> {
        let var_name = match target {
            Expression::Variable { name, .. } => name.clone(),
            _ => {
                return Err(EvalError::General {
                    message: "Assignment target must be a variable".into(),
                })
            }
        };

        let (target_reg, is_mutable) = self.lookup_var(var_name.as_str()).ok_or_else(|| {
            EvalError::UndefinedVariable {
                name: var_name.clone(),
            }
        })?;

        if !is_mutable {
            return Err(EvalError::ImmutableAssignment {
                name: var_name.clone(),
            });
        }

        let val_reg = self.compile_expression(value)?;
        match operator {
            AssignmentOperator::Assign => {
                self.chunk.emit(
                    OpCode::Move {
                        dst: target_reg,
                        src: val_reg,
                    },
                    span,
                );
            }
            AssignmentOperator::PlusAssign => {
                self.chunk.emit(
                    OpCode::Add {
                        dst: target_reg,
                        lhs: target_reg,
                        rhs: val_reg,
                    },
                    span,
                );
            }
            AssignmentOperator::MinusAssign => {
                self.chunk.emit(
                    OpCode::Sub {
                        dst: target_reg,
                        lhs: target_reg,
                        rhs: val_reg,
                    },
                    span,
                );
            }
            AssignmentOperator::StarAssign => {
                self.chunk.emit(
                    OpCode::Mul {
                        dst: target_reg,
                        lhs: target_reg,
                        rhs: val_reg,
                    },
                    span,
                );
            }
            AssignmentOperator::SlashAssign => {
                self.chunk.emit(
                    OpCode::Div {
                        dst: target_reg,
                        lhs: target_reg,
                        rhs: val_reg,
                    },
                    span,
                );
            }
            AssignmentOperator::PercentAssign => {
                self.chunk.emit(
                    OpCode::Mod {
                        dst: target_reg,
                        lhs: target_reg,
                        rhs: val_reg,
                    },
                    span,
                );
            }
        }
        Ok(())
    }

    fn compile_if_statement(
        &mut self,
        condition: &Expression,
        then_block: &Block,
        else_branch: &Option<ElseBranch>,
        span: Span,
    ) -> Result<(), EvalError> {
        let cond_reg = self.compile_expression(condition)?;
        let jump_false_idx = self.chunk.emit(
            OpCode::JumpIfFalse {
                cond: cond_reg,
                offset: JumpOffset(0),
            },
            span,
        );

        // Compile then block
        self.push_scope();
        for s in &then_block.statements {
            self.compile_statement(s)?;
        }
        if let Some(tail) = &then_block.tail_expr {
            self.compile_expression(tail)?;
        }
        self.pop_scope();

        if let Some(else_br) = else_branch {
            let jump_exit_idx = self.chunk.emit(
                OpCode::Jump {
                    offset: JumpOffset(0),
                },
                span,
            );

            // Patch jump_false to point here (start of else)
            let else_start = self.chunk.code.len();
            let offset = else_start as i32 - jump_false_idx as i32;
            self.chunk.code[jump_false_idx] = OpCode::JumpIfFalse {
                cond: cond_reg,
                offset: JumpOffset(offset),
            };

            self.push_scope();
            match else_br {
                ElseBranch::Block(b) => {
                    for s in &b.statements {
                        self.compile_statement(s)?;
                    }
                    if let Some(tail) = &b.tail_expr {
                        self.compile_expression(tail)?;
                    }
                }
                ElseBranch::ElseIf(s) => {
                    self.compile_statement(s)?;
                }
            }
            self.pop_scope();

            // Patch jump_exit to point to end
            let end_pos = self.chunk.code.len();
            let exit_offset = end_pos as i32 - jump_exit_idx as i32;
            self.chunk.code[jump_exit_idx] = OpCode::Jump {
                offset: JumpOffset(exit_offset),
            };
        } else {
            // Patch jump_false to point to end
            let end_pos = self.chunk.code.len();
            let offset = end_pos as i32 - jump_false_idx as i32;
            self.chunk.code[jump_false_idx] = OpCode::JumpIfFalse {
                cond: cond_reg,
                offset: JumpOffset(offset),
            };
        }
        Ok(())
    }

    fn compile_for_loop(
        &mut self,
        variables: &[CompactString],
        iterable: &Expression,
        body: &Block,
        span: Span,
    ) -> Result<(), EvalError> {
        let iter_reg = self.compile_expression(iterable)?;
        let index_reg = self.alloc_reg();
        self.chunk.emit(OpCode::LoadInt { dst: index_reg, val: 0 }, span);

        let loop_start_idx = self.chunk.code.len();

        // Push loop context
        self.loop_stack.push(super::LoopContext {
            loop_start_ip: loop_start_idx,
            step_ip: None,
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
        });

        // Get item at index
        let item_reg = self.alloc_reg();
        self.chunk.emit(
            OpCode::GetIndex {
                dst: item_reg,
                obj: iter_reg,
                index: index_reg,
            },
            span,
        );

        // Check if index reached end (GetIndex sets void or checks bounds)
        let is_void_reg = self.alloc_reg();
        let null_reg = self.alloc_reg();
        self.chunk.emit(OpCode::LoadNull { dst: null_reg }, span);
        self.chunk.emit(
            OpCode::Eq {
                dst: is_void_reg,
                lhs: item_reg,
                rhs: null_reg,
            },
            span,
        );

        let exit_jump_idx = self.chunk.emit(
            OpCode::JumpIfTrue {
                cond: is_void_reg,
                offset: JumpOffset(0),
            },
            span,
        );

        // Bind variable(s)
        self.push_scope();
        if variables.len() == 1 {
            self.bind_var(variables[0].clone(), item_reg, false);
        } else if variables.len() == 2 {
            let r0 = self.alloc_reg();
            let r1 = self.alloc_reg();
            let zero_reg = self.alloc_reg();
            let one_reg = self.alloc_reg();
            self.chunk.emit(OpCode::LoadInt { dst: zero_reg, val: 0 }, span);
            self.chunk.emit(OpCode::LoadInt { dst: one_reg, val: 1 }, span);
            self.chunk.emit(OpCode::GetIndex { dst: r0, obj: item_reg, index: zero_reg }, span);
            self.chunk.emit(OpCode::GetIndex { dst: r1, obj: item_reg, index: one_reg }, span);
            self.bind_var(variables[0].clone(), r0, false);
            self.bind_var(variables[1].clone(), r1, false);
        }

        // Compile loop body
        for s in &body.statements {
            self.compile_statement(s)?;
        }
        if let Some(tail) = &body.tail_expr {
            self.compile_expression(tail)?;
        }
        self.pop_scope();

        let step_pos = self.chunk.code.len();

        // Increment index
        let one = self.alloc_reg();
        self.chunk.emit(OpCode::LoadInt { dst: one, val: 1 }, span);
        self.chunk.emit(
            OpCode::Add {
                dst: index_reg,
                lhs: index_reg,
                rhs: one,
            },
            span,
        );

        // Jump back to loop start
        let cur_pos = self.chunk.code.len();
        let back_offset = loop_start_idx as i32 - cur_pos as i32;
        self.chunk.emit(OpCode::Jump { offset: JumpOffset(back_offset) }, span);

        // Patch exit jump
        let end_pos = self.chunk.code.len();
        let exit_offset = end_pos as i32 - exit_jump_idx as i32;
        self.chunk.code[exit_jump_idx] = OpCode::JumpIfTrue {
            cond: is_void_reg,
            offset: JumpOffset(exit_offset),
        };

        // Pop loop context and patch break/continue jumps
        let loop_ctx = self.loop_stack.pop().ok_or(EvalError::General { message: "Corrupted loop stack state".into() })?;
        for brk_idx in loop_ctx.break_jumps {
            let offset = end_pos as i32 - brk_idx as i32;
            self.chunk.code[brk_idx] = OpCode::Jump { offset: JumpOffset(offset) };
        }
        for cont_idx in loop_ctx.continue_jumps {
            let offset = step_pos as i32 - cont_idx as i32;
            self.chunk.code[cont_idx] = OpCode::Jump { offset: JumpOffset(offset) };
        }

        Ok(())
    }

    fn compile_match_statement(
        &mut self,
        target: &Expression,
        arms: &[MatchArm],
    ) -> Result<(), EvalError> {
        // 1. Evaluate target into a register
        let target_reg = self.compile_expression(target)?;
        let mut end_jumps = Vec::new();

        for arm in arms {
            let next_arm_jump = match &arm.pattern {
                Pattern::Wildcard { .. } => {
                    // Wildcard always matches; no test needed
                    None
                }
                Pattern::Expr(pattern_expr) => {
                    // Evaluate pattern expression into a temp register
                    let pattern_reg = self.compile_expression(pattern_expr)?;

                    // Compare target == pattern
                    let cond_reg = self.alloc_reg();
                    self.chunk.emit(
                        OpCode::Eq {
                            dst: cond_reg,
                            lhs: target_reg,
                            rhs: pattern_reg,
                        },
                        arm.span,
                    );

                    // Jump to next arm if not equal (placeholder offset)
                    let jump_idx = self.chunk.emit(
                        OpCode::JumpIfFalse {
                            cond: cond_reg,
                            offset: JumpOffset(0),
                        },
                        arm.span,
                    );
                    Some(jump_idx)
                }
            };

            // 2. Compile Arm Body
            self.push_scope();
            for s in &arm.body.statements {
                self.compile_statement(s)?;
            }
            // Compile tail expression (last expression without semicolon).
            // The parser stores it in `tail_expr`, not `statements`, so it
            // must be compiled explicitly — otherwise side-effecting calls
            // like `space.add_polygon(...)` are silently dropped.
            if let Some(tail) = &arm.body.tail_expr {
                self.compile_expression(tail)?;
            }
            self.pop_scope();

            // 3. Jump to end of match after executing arm
            let end_jump_idx = self.chunk.emit(
                OpCode::Jump {
                    offset: JumpOffset(0),
                },
                arm.span,
            );
            end_jumps.push(end_jump_idx);

            // 4. Backpatch the failure jump to the start of the next arm
            if let Some(idx) = next_arm_jump {
                let current_ip = self.chunk.code.len();
                let offset = current_ip as i32 - idx as i32;
                self.chunk.code[idx] = OpCode::JumpIfFalse {
                    cond: if let OpCode::JumpIfFalse { cond, .. } = self.chunk.code[idx] {
                        cond
                    } else {
                        Register(0)
                    },
                    offset: JumpOffset(offset),
                };
            }
        }

        // 5. Backpatch all arm completion jumps to point past the match statement
        let match_end_ip = self.chunk.code.len();
        for jump_idx in end_jumps {
            let offset = match_end_ip as i32 - jump_idx as i32;
            self.chunk.code[jump_idx] = OpCode::Jump {
                offset: JumpOffset(offset),
            };
        }

        Ok(())
    }

    fn compile_route_statement(
        &mut self,
        from: &Expression,
        to: &Expression,
        intent: &Option<CompactString>,
        body: &Option<Block>,
        span: Span,
    ) -> Result<(), EvalError> {
        let from_reg = self.compile_expression(from)?;
        let to_reg = self.compile_expression(to)?;
        let mut final_intent = intent.as_ref().map(|s| s.as_str()).unwrap_or("default").to_string();

        // Compile route body properties into struct
        let props_reg = if let Some(blk) = body {
            let mut prop_regs = Vec::new();
            let mut field_names = Vec::new();
            for s in &blk.statements {
                if let Statement::Let { pattern: BindingPattern::Identifier(name), value, .. } = s {
                    if name.as_str() == "intent" {
                        if let Expression::FieldAccess { field: variant_name, .. } = value {
                            final_intent = variant_name.to_string();
                            continue;
                        } else if let Expression::Path { segments, .. } = value {
                            if let Some(variant) = segments.last() {
                                final_intent = variant.to_string();
                                continue;
                            }
                        } else if let Expression::Variable { name: intent_val, .. } = value {
                            final_intent = intent_val.to_string();
                            continue;
                        } else if let Expression::StringLiteral { value: intent_val, .. } = value {
                            final_intent = intent_val.clone();
                            continue;
                        }
                    }

                    let val_r = self.compile_expression(value)?;
                    prop_regs.push(val_r);
                    field_names.push(name.clone());
                }
            }
            if !prop_regs.is_empty() {
                let start_r = self.alloc_reg();
                for (i, r) in prop_regs.iter().enumerate() {
                    let target_r = if i == 0 { start_r } else { self.alloc_reg() };
                    self.chunk.emit(OpCode::Move { dst: target_r, src: *r }, span);
                }
                let struct_meta = Value::StructInstance {
                    name: "RouteProps".into(),
                    fields: std::sync::Arc::new(field_names.into_iter().map(|f| (f, Value::Void)).collect()),
                };
                let name_const = self.chunk.add_constant(struct_meta);
                let dst_r = self.alloc_reg();
                self.chunk.emit(
                    OpCode::AllocStruct {
                        dst: dst_r,
                        struct_name_idx: name_const,
                        fields_start: start_r,
                        count: prop_regs.len() as u16,
                    },
                    span,
                );
                dst_r
            } else {
                let r = self.alloc_reg();
                self.chunk.emit(OpCode::LoadNull { dst: r }, span);
                r
            }
        } else {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        };

        let intent_const = self.chunk.add_constant(Value::String(final_intent.into()));
        self.chunk.emit(
            OpCode::EmitRoute {
                from_reg,
                to_reg,
                intent_idx: intent_const,
                props_reg,
            },
            span,
        );
        Ok(())
    }
}
