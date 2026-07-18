use hwc_parser::expr::{BinOp, Expr, UnaryOp};

pub struct EvalEnv {
    pub params: Vec<(String, f64)>,
    pub vars: rustc_hash::FxHashMap<String, f64>,
}

impl EvalEnv {
    pub fn get(&self, name: &str) -> Option<f64> {
        self.vars
            .get(name)
            .copied()
            .or_else(|| self.params.iter().find(|(k, _)| k == name).map(|(_, v)| *v))
    }
}

pub fn evaluate_expr(expr: &Expr, env: &EvalEnv) -> f64 {
    match expr {
        Expr::Literal(val) => *val,
        Expr::Identifier(name) => env.get(name).unwrap_or(0.0),
        Expr::UnaryOp { op, expr } => {
            let val = evaluate_expr(expr, env);
            match op {
                UnaryOp::Pos => val,
                UnaryOp::Neg => -val,
            }
        }
        Expr::BinOp { op, left, right } => {
            let l = evaluate_expr(left, env);
            let r = evaluate_expr(right, env);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r == 0.0 {
                        0.0
                    } else {
                        l / r
                    }
                }
                BinOp::Mod => {
                    if r == 0.0 {
                        0.0
                    } else {
                        l % r
                    }
                }
                BinOp::Eq => {
                    if l == r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Ne => {
                    if l != r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Lt => {
                    if l < r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Gt => {
                    if l > r {
                        1.0
                    } else {
                        0.0
                    }
                }
            }
        }
        Expr::Call { name, args } => {
            let arg_val = args.first().map(|a| evaluate_expr(a, env)).unwrap_or(0.0);
            match name.as_str() {
                "sin" => arg_val.to_radians().sin(),
                "cos" => arg_val.to_radians().cos(),
                "tan" => arg_val.to_radians().tan(),
                _ => 0.0,
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let cond_val = evaluate_expr(cond, env);
            if cond_val != 0.0 {
                evaluate_expr(then_branch, env)
            } else {
                evaluate_expr(else_branch, env)
            }
        }
        Expr::ModEquals {
            dividend,
            divisor,
            remainder,
        } => {
            let d = evaluate_expr(dividend, env);
            let m = evaluate_expr(divisor, env);
            let r = evaluate_expr(remainder, env);
            if m != 0.0 && (d % m) == r {
                1.0
            } else {
                0.0
            }
        }
        Expr::Constant(name) => env.get(name).unwrap_or(0.0),
    }
}
