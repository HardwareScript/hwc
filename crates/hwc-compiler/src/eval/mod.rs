//! HardwareScript v0.3.0 Comptime Evaluation Engine (`hwc-eval`)

pub mod builtins;
pub mod bytecode;
pub mod context;
pub mod emitter;
pub mod escape_contract;
pub mod sandbox;
pub mod value;

pub use context::{Binding, EvalError, EvaluationContext, ScopeFrame};
pub use emitter::{
    ContactRecord, DeviceRecord, MemoryEmitter, PolygonRecord, RouteRecord, SpaceEmitter,
};
pub use escape_contract::EscapeEnvelope;
pub use sandbox::{SandboxGuard, MAX_EVAL_STEPS, MAX_RECURSION_DEPTH};
pub use value::{
    DeviceId, FunctionId, MeasurementValue, PhysicalDimension, PhysicalValue, SpaceId,
    UnitDimension, Value,
};

use compact_str::CompactString;
use hwc_parser::ast::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Control flow signals during statement execution
#[derive(Debug, Clone, PartialEq)]
pub enum ControlFlow {
    Continue,
    Return(Value),
}

/// Main Evaluator for HardwareScript v0.3.0 AST
pub struct Evaluator<'a> {
    pub ctx: &'a mut EvaluationContext,
}

impl<'a> Evaluator<'a> {
    pub fn new(ctx: &'a mut EvaluationContext) -> Self {
        Self { ctx }
    }

    /// Evaluate an entire Program AST
    pub fn eval_program(&mut self, program: &Program) -> Result<(), EvalError> {
        // Pass 1: Register all top-level functions and structs
        for item in &program.items {
            match item {
                TopLevelItem::Function(f) => {
                    self.ctx
                        .functions
                        .insert(f.name.name.clone(), f.clone());
                }
                TopLevelItem::Struct(s) => {
                    self.ctx.structs.insert(s.name.name.clone(), s.clone());
                }
                _ => {}
            }
        }

        // Pass 2: Evaluate space blocks and standalone statements
        let mut space_counter = 1;
        for item in &program.items {
            if let TopLevelItem::Space(space) = item {
                self.eval_space(space, space_counter)?;
                space_counter += 1;
            }
        }

        Ok(())
    }

    /// Evaluate a `space` block
    pub fn eval_space(&mut self, space: &SpaceDecl, space_id: u32) -> Result<(), EvalError> {
        self.ctx.enter_space(space_id);

        // 1. Evaluate `nets { ... }` block
        for net_decl in &space.nets {
            let mut props = FxHashMap::default();
            for (prop_name, prop_expr) in &net_decl.properties {
                let val = self.eval_expression(prop_expr)?;
                props.insert(prop_name.clone(), val);
            }
            let net_id = self.ctx.emitter.allocate_net(
                space_id,
                net_decl.name.as_str(),
                props,
            )?;
            // Inject strongly-typed NetHandle into local space scope
            self.ctx
                .bind(net_decl.name.as_str(), Value::NetHandle(net_id), false);
        }

        // 2. Evaluate space statements
        for stmt in &space.statements {
            let flow = self.eval_statement(stmt)?;
            if let ControlFlow::Return(_) = flow {
                break;
            }
        }

        self.ctx.exit_space();
        Ok(())
    }

    /// Evaluate a block of statements
    pub fn eval_block(&mut self, block: &Block) -> Result<ControlFlow, EvalError> {
        self.ctx.push_scope();

        for stmt in &block.statements {
            self.ctx.sandbox.tick()?;
            let flow = self.eval_statement(stmt)?;
            if let ControlFlow::Return(_) = flow {
                self.ctx.pop_scope();
                return Ok(flow);
            }
        }

        self.ctx.pop_scope();
        Ok(ControlFlow::Continue)
    }

    /// Evaluate a single statement
    pub fn eval_statement(&mut self, stmt: &Statement) -> Result<ControlFlow, EvalError> {
        self.ctx.sandbox.tick()?;

        match stmt {
            Statement::Let {
                mutable,
                name,
                type_annotation,
                value,
                ..
            } => {
                let evaluated = self.eval_expression(value)?;
                let coerced = if let Some(type_expr) = type_annotation {
                    if let TypeExpr::Named { name: type_name, .. } = type_expr {
                        evaluated.coerce_to_type(type_name.as_str())?
                    } else {
                        evaluated
                    }
                } else {
                    evaluated
                };
                self.ctx.bind(name.as_str(), coerced, *mutable);
                Ok(ControlFlow::Continue)
            }

            Statement::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                let new_val = self.eval_expression(value)?;
                match target {
                    Expression::Variable { name, .. } => {
                        let final_val = match operator {
                            AssignmentOperator::Assign => new_val,
                            AssignmentOperator::PlusAssign => {
                                let cur = self.ctx.lookup(name.as_str()).ok_or_else(|| {
                                    EvalError::UndefinedVariable { name: name.clone() }
                                })?;
                                cur.add(&new_val)?
                            }
                            AssignmentOperator::MinusAssign => {
                                let cur = self.ctx.lookup(name.as_str()).ok_or_else(|| {
                                    EvalError::UndefinedVariable { name: name.clone() }
                                })?;
                                cur.sub(&new_val)?
                            }
                            AssignmentOperator::StarAssign => {
                                let cur = self.ctx.lookup(name.as_str()).ok_or_else(|| {
                                    EvalError::UndefinedVariable { name: name.clone() }
                                })?;
                                cur.mul(&new_val)?
                            }
                            AssignmentOperator::SlashAssign => {
                                let cur = self.ctx.lookup(name.as_str()).ok_or_else(|| {
                                    EvalError::UndefinedVariable { name: name.clone() }
                                })?;
                                cur.div(&new_val)?
                            }
                        };
                        self.ctx.assign(name.as_str(), final_val)?;
                        Ok(ControlFlow::Continue)
                    }
                    _ => Err(EvalError::General {
                        message: "Assignment target must be a variable".into(),
                    }),
                }
            }

            Statement::If {
                condition,
                then_block,
                else_branch,
                ..
            } => {
                let cond_val = self.eval_expression(condition)?;
                let is_true = match cond_val {
                    Value::Bool(b) => b,
                    Value::Int(i) => i != 0,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Bool",
                            found: other.type_name().to_string(),
                        })
                    }
                };

                if is_true {
                    self.eval_block(then_block)
                } else if let Some(else_br) = else_branch {
                    match else_br {
                        ElseBranch::Block(blk) => self.eval_block(blk),
                        ElseBranch::ElseIf(else_if_stmt) => self.eval_statement(else_if_stmt),
                    }
                } else {
                    Ok(ControlFlow::Continue)
                }
            }

            Statement::For {
                variables,
                iterable,
                body,
                ..
            } => {
                let iter_val = self.eval_expression(iterable)?;
                let items: Arc<Vec<Value>> = match iter_val {
                    Value::Array(arr) => arr,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Array or Range",
                            found: other.type_name().to_string(),
                        })
                    }
                };

                for item in items.iter() {
                    self.ctx.sandbox.tick()?;
                    self.ctx.push_scope();

                    if variables.len() == 1 {
                        self.ctx.bind(variables[0].as_str(), item.clone(), false);
                    } else if variables.len() == 2 {
                        if let Value::Array(pair) = item {
                            if pair.len() >= 2 {
                                self.ctx
                                    .bind(variables[0].as_str(), pair[0].clone(), false);
                                self.ctx
                                    .bind(variables[1].as_str(), pair[1].clone(), false);
                            }
                        }
                    }

                    for s in &body.statements {
                        let flow = self.eval_statement(s)?;
                        if let ControlFlow::Return(_) = flow {
                            self.ctx.pop_scope();
                            return Ok(flow);
                        }
                    }

                    self.ctx.pop_scope();
                }

                Ok(ControlFlow::Continue)
            }

            Statement::Return { value, .. } => {
                let val = if let Some(v) = value {
                    self.eval_expression(v)?
                } else {
                    Value::Void
                };
                Ok(ControlFlow::Return(val))
            }

            Statement::Assert {
                condition,
                message,
                ..
            } => {
                let cond_val = self.eval_expression(condition)?;
                let is_true = match cond_val {
                    Value::Bool(b) => b,
                    Value::Int(i) => i != 0,
                    _ => false,
                };
                if !is_true {
                    let msg = message.as_deref().unwrap_or("Assertion failed");
                    return Err(EvalError::AssertionFailed {
                        message: msg.to_string(),
                    });
                }
                Ok(ControlFlow::Continue)
            }

            Statement::Expression { expression, .. } => {
                self.eval_expression(expression)?;
                Ok(ControlFlow::Continue)
            }

            Statement::Route {
                from,
                to,
                intent,
                body,
                ..
            } => {
                let space_id = self.ctx.current_space_id.ok_or(
                    EvalError::NoActiveSpaceContext { method: "route" },
                )?;
                let from_val = self.eval_expression(from)?;
                let to_val = self.eval_expression(to)?;

                let mut props = FxHashMap::default();
                if let Some(blk) = body {
                    for s in &blk.statements {
                        if let Statement::Let { name, value, .. } = s {
                            let val = self.eval_expression(value)?;
                            props.insert(name.clone(), val);
                        }
                    }
                }

                self.ctx.emitter.add_route(
                    space_id,
                    from_val,
                    to_val,
                    intent.as_ref().map(|s| s.as_str().into()),
                    props,
                )?;

                Ok(ControlFlow::Continue)
            }
        }
    }

    /// Evaluate an Expression AST node
    pub fn eval_expression(&mut self, expr: &Expression) -> Result<Value, EvalError> {
        self.ctx.sandbox.tick()?;

        match expr {
            Expression::Literal { value, .. } => Ok(Value::Int(*value)),
            Expression::FloatLiteral { value, .. } => Ok(Value::Float(*value)),
            Expression::BooleanLiteral { value, .. } => Ok(Value::Bool(*value)),
            Expression::StringLiteral { value, .. } => {
                // Perform string interpolation if `{...}` is present
                if value.contains('{') && value.contains('}') {
                    let mut rendered = String::new();
                    let mut chars = value.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '{' {
                            let mut var_name = String::new();
                            for inner in chars.by_ref() {
                                if inner == '}' {
                                    break;
                                }
                                var_name.push(inner);
                            }
                            if let Some(val) = self.ctx.lookup(var_name.trim()) {
                                rendered.push_str(&format!("{}", val));
                            } else {
                                rendered.push('{');
                                rendered.push_str(&var_name);
                                rendered.push('}');
                            }
                        } else {
                            rendered.push(c);
                        }
                    }
                    Ok(Value::String(rendered.into()))
                } else {
                    Ok(Value::String(value.as_str().into()))
                }
            }

            Expression::Measurement { value, unit, .. } => {
                if let Some(phys) = MeasurementValue::from_ast_unit(
                    *value,
                    unit,
                    self.ctx.unit_registry.as_deref(),
                ) {
                    Ok(Value::Measurement(phys))
                } else {
                    Ok(Value::Float(*value))
                }
            }

            Expression::Variable { name, .. } => {
                if let Some(val) = self.ctx.lookup(name.as_str()) {
                    Ok(val)
                } else {
                    Err(EvalError::UndefinedVariable { name: name.clone() })
                }
            }

            Expression::ArrayLiteral { elements, .. } => {
                let mut vals = Vec::new();
                for e in elements {
                    vals.push(self.eval_expression(e)?);
                }
                Ok(Value::Array(Arc::new(vals)))
            }

            Expression::StructInstance { name, fields, .. } => {
                let mut list = Vec::new();
                for f in fields {
                    let val = if let Some(v_expr) = &f.value {
                        self.eval_expression(v_expr)?
                    } else {
                        // Shorthand field lookup
                        self.ctx.lookup(f.name.as_str()).ok_or_else(|| {
                            EvalError::UndefinedVariable {
                                name: f.name.clone(),
                            }
                        })?
                    };
                    list.push((f.name.clone(), val));
                }
                Ok(Value::StructInstance {
                    name: name.clone(),
                    fields: Arc::new(list),
                })
            }

            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let l_val = self.eval_expression(left)?;
                let r_val = self.eval_expression(right)?;

                match operator {
                    BinaryOperator::Add => l_val.add(&r_val),
                    BinaryOperator::Subtract => l_val.sub(&r_val),
                    BinaryOperator::Multiply => l_val.mul(&r_val),
                    BinaryOperator::Divide => l_val.div(&r_val),
                    BinaryOperator::Modulo => l_val.modulo(&r_val),
                    BinaryOperator::Equal => Ok(Value::Bool(l_val == r_val)),
                    BinaryOperator::NotEqual => Ok(Value::Bool(l_val != r_val)),
                    BinaryOperator::LessThan => self.eval_comparison(&l_val, &r_val, |a, b| a < b),
                    BinaryOperator::GreaterThan => {
                        self.eval_comparison(&l_val, &r_val, |a, b| a > b)
                    }
                    BinaryOperator::LessThanOrEqual => {
                        self.eval_comparison(&l_val, &r_val, |a, b| a <= b)
                    }
                    BinaryOperator::GreaterThanOrEqual => {
                        self.eval_comparison(&l_val, &r_val, |a, b| a >= b)
                    }
                    BinaryOperator::And => {
                        let a = match l_val {
                            Value::Bool(b) => b,
                            Value::Int(i) => i != 0,
                            _ => false,
                        };
                        let b = match r_val {
                            Value::Bool(b) => b,
                            Value::Int(i) => i != 0,
                            _ => false,
                        };
                        Ok(Value::Bool(a && b))
                    }
                    BinaryOperator::Or => {
                        let a = match l_val {
                            Value::Bool(b) => b,
                            Value::Int(i) => i != 0,
                            _ => false,
                        };
                        let b = match r_val {
                            Value::Bool(b) => b,
                            Value::Int(i) => i != 0,
                            _ => false,
                        };
                        Ok(Value::Bool(a || b))
                    }
                }
            }

            Expression::Unary {
                operator,
                operand,
                ..
            } => {
                let val = self.eval_expression(operand)?;
                match operator {
                    UnaryOperator::Not => match val {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        Value::Int(i) => Ok(Value::Bool(i == 0)),
                        other => Err(EvalError::TypeMismatch {
                            expected: "Bool",
                            found: other.type_name().to_string(),
                        }),
                    },
                    UnaryOperator::Negate => val.neg(),
                    UnaryOperator::Plus => Ok(val),
                }
            }

            Expression::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let s_val = self.eval_expression(start)?;
                let e_val = self.eval_expression(end)?;

                let s = match s_val {
                    Value::Int(i) => i,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Int for range start",
                            found: other.type_name().to_string(),
                        })
                    }
                };
                let e = match e_val {
                    Value::Int(i) => i,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Int for range end",
                            found: other.type_name().to_string(),
                        })
                    }
                };

                let range_vec: Vec<Value> = if *inclusive {
                    (s..=e).map(Value::Int).collect()
                } else {
                    (s..e).map(Value::Int).collect()
                };

                Ok(Value::Array(Arc::new(range_vec)))
            }

            Expression::Call {
                callee,
                arguments,
                ..
            } => {
                // 1. Check if method call on `space.*`
                if let Expression::FieldAccess { target, field, .. } = callee.as_ref() {
                    if let Expression::Variable { name, .. } = target.as_ref() {
                        if name.as_str() == "space" {
                            return self.eval_space_method(field.as_str(), arguments);
                        }
                    }
                }

                // 2. Check built-in functions (println, max, min, abs, rect_between)
                if let Expression::Variable { name, .. } = callee.as_ref() {
                    if builtins::is_builtin(name.as_str()) {
                        let mut arg_vals = Vec::new();
                        for arg in arguments {
                            arg_vals.push(self.eval_expression(&arg.value)?);
                        }
                        return builtins::call_builtin(name.as_str(), arg_vals);
                    }
                }

                // 3. User-defined function call
                let fn_name = match callee.as_ref() {
                    Expression::Variable { name, .. } => name.clone(),
                    _ => {
                        return Err(EvalError::General {
                            message: "Call target must be a function name or method".into(),
                        })
                    }
                };

                let func_decl = self.ctx.functions.get(&fn_name).cloned().ok_or_else(|| {
                    EvalError::UnknownFunction {
                        name: fn_name.clone(),
                    }
                })?;

                self.ctx
                    .sandbox
                    .check_recursion_depth(self.ctx.scopes.len())?;

                // Evaluate arguments and bind parameters into new call frame
                let mut param_map = FxHashMap::default();
                for (i, arg) in arguments.iter().enumerate() {
                    let evaluated_arg = self.eval_expression(&arg.value)?;
                    if let Some(param_name) = &arg.name {
                        param_map.insert(param_name.clone(), evaluated_arg);
                    } else if i < func_decl.parameters.len() {
                        param_map.insert(func_decl.parameters[i].name.clone(), evaluated_arg);
                    }
                }

                self.ctx.push_scope();

                for param in &func_decl.parameters {
                    let arg_val = if let Some(val) = param_map.remove(&param.name) {
                        // Coerce if parameter type is Point2D
                        if let TypeExpr::Named {
                            name: type_name, ..
                        } = &param.type_annotation
                        {
                            val.coerce_to_type(type_name.as_str())?
                        } else {
                            val
                        }
                    } else if let Some(def_expr) = &param.default_value {
                        self.eval_expression(def_expr)?
                    } else {
                        self.ctx.pop_scope();
                        return Err(EvalError::MissingArgument {
                            param: param.name.clone(),
                            func: fn_name,
                        });
                    };

                    self.ctx.bind(param.name.as_str(), arg_val, false);
                }

                // Execute function body
                let mut return_val = Value::Void;
                for stmt in &func_decl.body.statements {
                    let flow = self.eval_statement(stmt)?;
                    if let ControlFlow::Return(val) = flow {
                        return_val = val;
                        break;
                    }
                }

                self.ctx.pop_scope();
                Ok(return_val)
            }

            Expression::FieldAccess { target, field, .. } => {
                let target_val = self.eval_expression(target)?;
                match target_val {
                    Value::StructInstance { fields, name } => {
                        for (k, v) in fields.iter() {
                            if k.as_str() == field.as_str() {
                                return Ok(v.clone());
                            }
                        }
                        Err(EvalError::FieldNotFound {
                            field: field.clone(),
                            struct_name: name,
                        })
                    }
                    Value::Point2D { x, y } => match field.as_str() {
                        "x" => Ok(Value::Measurement(MeasurementValue::length_pm(x as i128))),
                        "y" => Ok(Value::Measurement(MeasurementValue::length_pm(y as i128))),
                        _ => Err(EvalError::General {
                            message: format!("Point2D only has fields 'x' and 'y', found '{}'", field),
                        }),
                    },
                    other => Err(EvalError::TypeMismatch {
                        expected: "StructInstance or Point2D",
                        found: other.type_name().to_string(),
                    }),
                }
            }

            Expression::Index { target, index, .. } => {
                let target_val = self.eval_expression(target)?;
                let index_val = self.eval_expression(index)?;

                match (target_val, index_val) {
                    (Value::Array(items), Value::Int(i)) => {
                        let idx = if i < 0 {
                            (items.len() as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        items
                            .get(idx)
                            .cloned()
                            .ok_or(EvalError::IndexOutOfBounds {
                                index: i,
                                len: items.len(),
                            })
                    }
                    (a, b) => Err(EvalError::TypeMismatch {
                        expected: "Array and Int for indexing",
                        found: format!("{} and {}", a.type_name(), b.type_name()),
                    }),
                }
            }

            Expression::Grouped { expression, .. } => self.eval_expression(expression),
        }
    }

    /// Evaluate native physical emitter methods: `space.add_polygon`, `space.add_contact`, `space.add_device`
    fn eval_space_method(
        &mut self,
        method: &str,
        args: &[NamedOrPositionalArg],
    ) -> Result<Value, EvalError> {
        let space_id = self
            .ctx
            .current_space_id
            .ok_or(EvalError::NoActiveSpaceContext { method: "emitter" })?;

        let mut named_args = FxHashMap::default();
        for arg in args {
            let val = self.eval_expression(&arg.value)?;
            if let Some(name) = &arg.name {
                named_args.insert(name.clone(), val);
            }
        }

        match method {
            "add_polygon" => {
                let layer = match named_args.get("layer") {
                    Some(Value::String(s)) => s.as_str(),
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "String for layer",
                            found: other.map(|v| v.type_name()).unwrap_or("None").to_string(),
                        })
                    }
                };
                let net = match named_args.get("net") {
                    Some(Value::NetHandle(id)) => Some(*id),
                    _ => None,
                };

                let points = if let Some(rect_val) = named_args.get("rect") {
                    if let Value::Array(r) = rect_val {
                        if r.len() == 4 {
                            let x = match &r[0] {
                                Value::Measurement(m) => m.raw as i64,
                                Value::Int(i) => *i,
                                _ => {
                                    return Err(EvalError::TypeMismatch {
                                        expected: "Length for rect[0] (x)",
                                        found: r[0].type_name().to_string(),
                                    })
                                }
                            };
                            let y = match &r[1] {
                                Value::Measurement(m) => m.raw as i64,
                                Value::Int(i) => *i,
                                _ => {
                                    return Err(EvalError::TypeMismatch {
                                        expected: "Length for rect[1] (y)",
                                        found: r[1].type_name().to_string(),
                                    })
                                }
                            };
                            let w = match &r[2] {
                                Value::Measurement(m) => m.raw as i64,
                                Value::Int(i) => *i,
                                _ => {
                                    return Err(EvalError::TypeMismatch {
                                        expected: "Length for rect[2] (w)",
                                        found: r[2].type_name().to_string(),
                                    })
                                }
                            };
                            let h = match &r[3] {
                                Value::Measurement(m) => m.raw as i64,
                                Value::Int(i) => *i,
                                _ => {
                                    return Err(EvalError::TypeMismatch {
                                        expected: "Length for rect[3] (h)",
                                        found: r[3].type_name().to_string(),
                                    })
                                }
                            };
                            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                        } else {
                            return Err(EvalError::General {
                                message: "space.add_polygon 'rect' array must have 4 elements [x, y, w, h]".into(),
                            });
                        }
                    } else {
                        return Err(EvalError::TypeMismatch {
                            expected: "Array for rect",
                            found: rect_val.type_name().to_string(),
                        });
                    }
                } else if let Some(points_val) = named_args.get("points") {
                    if let Value::Array(pts) = points_val {
                        let mut poly_pts = Vec::new();
                        for p in pts.iter() {
                            let coerced = p.coerce_to_point2d()?;
                            if let Value::Point2D { x, y } = coerced {
                                poly_pts.push((x, y));
                            }
                        }
                        poly_pts
                    } else {
                        return Err(EvalError::TypeMismatch {
                            expected: "Array of Point2D for points",
                            found: points_val.type_name().to_string(),
                        });
                    }
                } else {
                    return Err(EvalError::General {
                        message: "space.add_polygon requires either 'rect' or 'points'".into(),
                    });
                };

                self.ctx.emitter.add_polygon(space_id, layer, net, points)?;
                Ok(Value::Void)
            }

            "add_contact" => {
                let from = match named_args.get("from") {
                    Some(Value::String(s)) => s.clone(),
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "String for from layer",
                            found: other.map(|v| v.type_name()).unwrap_or("None").to_string(),
                        })
                    }
                };
                let to = match named_args.get("to") {
                    Some(Value::String(s)) => s.clone(),
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "String for to layer",
                            found: other.map(|v| v.type_name()).unwrap_or("None").to_string(),
                        })
                    }
                };
                let at_p = match named_args.get("at") {
                    Some(val) => val.coerce_to_point2d()?,
                    None => {
                        return Err(EvalError::General {
                            message: "space.add_contact requires 'at: Point2D'".into(),
                        })
                    }
                };
                let at = match at_p {
                    Value::Point2D { x, y } => (x, y),
                    _ => unreachable!(),
                };
                let diameter_pm = match named_args.get("diameter") {
                    Some(Value::Measurement(m)) if m.dimension == UnitDimension::Length => {
                        m.raw as i64
                    }
                    Some(Value::Int(i)) => *i,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Length measurement for diameter",
                            found: other.map(|v| v.type_name()).unwrap_or("None").to_string(),
                        })
                    }
                };
                let net = match named_args.get("net") {
                    Some(Value::NetHandle(id)) => Some(*id),
                    _ => None,
                };

                self.ctx.emitter.add_contact(
                    space_id,
                    from.as_str(),
                    to.as_str(),
                    at,
                    diameter_pm,
                    net,
                )?;
                Ok(Value::Void)
            }

            "add_device" => {
                let dev_type = match named_args.get("type") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::EnumVariant { variant_name, .. }) => variant_name.clone(),
                    _ => CompactString::new("NMOS"),
                };
                let name = match named_args.get("name") {
                    Some(Value::String(s)) => s.clone(),
                    _ => CompactString::new("DEV"),
                };
                let terminals = match named_args.get("terminals") {
                    Some(Value::StructInstance { fields, .. }) => {
                        let mut map = FxHashMap::default();
                        for (k, v) in fields.iter() {
                            if let Value::NetHandle(id) = v {
                                map.insert(k.clone(), *id);
                            }
                        }
                        map
                    }
                    _ => FxHashMap::default(),
                };
                let params = match named_args.get("params") {
                    Some(Value::StructInstance { fields, .. }) => {
                        let mut map = FxHashMap::default();
                        for (k, v) in fields.iter() {
                            if let Value::Measurement(m) = v {
                                map.insert(k.clone(), *m);
                            }
                        }
                        map
                    }
                    _ => FxHashMap::default(),
                };

                self.ctx.emitter.add_device(
                    space_id,
                    dev_type.as_str(),
                    name.as_str(),
                    terminals,
                    params,
                )?;
                Ok(Value::Void)
            }

            _ => Err(EvalError::General {
                message: format!("Unknown space method 'space.{}'", method),
            }),
        }
    }

    fn eval_comparison<F>(&self, left: &Value, right: &Value, cmp: F) -> Result<Value, EvalError>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(cmp(*a as f64, *b as f64))),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(cmp(*a, *b))),
            (Value::Measurement(a), Value::Measurement(b)) => {
                if a.dimension != b.dimension {
                    return Err(EvalError::UnitMismatch {
                        expected: a.dimension,
                        found: b.dimension,
                        op: "comparison",
                    });
                }
                Ok(Value::Bool(cmp(a.raw as f64, b.raw as f64)))
            }
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Comparable numeric types",
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }
}
