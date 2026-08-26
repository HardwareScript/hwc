//! HardwareScript v0.3.0 Standard Built-In Functions

use super::context::EvalError;
use super::value::{MeasurementValue, UnitDimension, Value};

/// Check if a name is a standard built-in function
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "println"
            | "eprintln"
            | "dbg"
            | "assert"
            | "min"
            | "max"
            | "abs"
            | "sin"
            | "cos"
            | "tan"
            | "sqrt"
            | "rect_between"
            | "pad"
    )
}

/// Call a built-in function with arguments
pub fn call_builtin(name: &str, args: Vec<Value>) -> Result<Value, EvalError> {
    match name {
        "println" => {
            if let Some(first) = args.first() {
                if let Value::String(fmt_str) = first {
                    let mut rendered = fmt_str.to_string();
                    for (i, arg) in args.iter().skip(1).enumerate() {
                        let placeholder = format!("{{{}}}", i);
                        rendered = rendered.replace(&placeholder, &format!("{}", arg));
                    }
                    println!("{}", rendered);
                } else {
                    let items: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                    println!("{}", items.join(" "));
                }
            } else {
                println!();
            }
            Ok(Value::Void)
        }

        "eprintln" => {
            let items: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            eprintln!("{}", items.join(" "));
            Ok(Value::Void)
        }

        "dbg" => {
            if let Some(val) = args.first() {
                eprintln!("[DEBUG] {:?}", val);
                Ok(val.clone())
            } else {
                Ok(Value::Void)
            }
        }

        "assert" => {
            let cond_true = match args.first() {
                Some(Value::Bool(b)) => *b,
                Some(Value::Int(i)) => *i != 0,
                _ => false,
            };
            if !cond_true {
                let msg = if args.len() > 1 {
                    format!("{}", args[1])
                } else {
                    "Assertion failed".into()
                };
                Err(EvalError::AssertionFailed { message: msg })
            } else {
                Ok(Value::Void)
            }
        }

        "max" => {
            if args.len() < 2 {
                return Err(EvalError::General {
                    message: "max() requires at least 2 arguments".into(),
                });
            }
            let mut current = args[0].clone();
            for item in &args[1..] {
                match (&current, item) {
                    (Value::Int(a), Value::Int(b)) => {
                        if b > a {
                            current = Value::Int(*b);
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if b > a {
                            current = Value::Float(*b);
                        }
                    }
                    (Value::Measurement(a), Value::Measurement(b)) => {
                        if a.dimension != b.dimension {
                            return Err(EvalError::UnitMismatch {
                                expected: a.dimension,
                                found: b.dimension,
                                op: "max",
                            });
                        }
                        if b.raw > a.raw {
                            current = Value::Measurement(*b);
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Comparable types for max()",
                            found: format!("{} and {}", current.type_name(), item.type_name()),
                        })
                    }
                }
            }
            Ok(current)
        }

        "min" => {
            if args.len() < 2 {
                return Err(EvalError::General {
                    message: "min() requires at least 2 arguments".into(),
                });
            }
            let mut current = args[0].clone();
            for item in &args[1..] {
                match (&current, item) {
                    (Value::Int(a), Value::Int(b)) => {
                        if b < a {
                            current = Value::Int(*b);
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if b < a {
                            current = Value::Float(*b);
                        }
                    }
                    (Value::Measurement(a), Value::Measurement(b)) => {
                        if a.dimension != b.dimension {
                            return Err(EvalError::UnitMismatch {
                                expected: a.dimension,
                                found: b.dimension,
                                op: "min",
                            });
                        }
                        if b.raw < a.raw {
                            current = Value::Measurement(*b);
                        }
                    }
                    _ => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Comparable types for min()",
                            found: format!("{} and {}", current.type_name(), item.type_name()),
                        })
                    }
                }
            }
            Ok(current)
        }

        "abs" => {
            if let Some(arg) = args.first() {
                match arg {
                    Value::Int(i) => Ok(Value::Int(i.abs())),
                    Value::Float(f) => Ok(Value::Float(f.abs())),
                    Value::Measurement(m) => Ok(Value::Measurement(MeasurementValue::new(
                        m.raw.abs(),
                        m.dimension,
                    ))),
                    other => Err(EvalError::TypeMismatch {
                        expected: "Numeric type for abs()",
                        found: other.type_name().to_string(),
                    }),
                }
            } else {
                Err(EvalError::General {
                    message: "abs() requires 1 argument".into(),
                })
            }
        }

        "sqrt" => {
            if let Some(arg) = args.first() {
                match arg {
                    Value::Float(f) => {
                        if *f < 0.0 {
                            Err(EvalError::General {
                                message: "sqrt() domain error: cannot compute square root of negative number".into(),
                            })
                        } else {
                            Ok(Value::Float(f.sqrt()))
                        }
                    }
                    Value::Int(i) => {
                        if *i < 0 {
                            Err(EvalError::General {
                                message: "sqrt() domain error: cannot compute square root of negative number".into(),
                            })
                        } else {
                            Ok(Value::Float((*i as f64).sqrt()))
                        }
                    }
                    other => Err(EvalError::TypeMismatch {
                        expected: "Float or Int for sqrt()",
                        found: other.type_name().to_string(),
                    }),
                }
            } else {
                Err(EvalError::General {
                    message: "sqrt() requires 1 argument".into(),
                })
            }
        }

        "sin" => {
            if let Some(arg) = args.first() {
                let rad = match arg {
                    Value::Float(f) => *f,
                    Value::Measurement(m) if m.dimension == UnitDimension::Angle => {
                        (m.raw as f64 / 1_000_000.0).to_radians()
                    }
                    Value::Int(i) => *i as f64,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Angle or Float for sin()",
                            found: other.type_name().to_string(),
                        })
                    }
                };
                Ok(Value::Float(rad.sin()))
            } else {
                Err(EvalError::General {
                    message: "sin() requires 1 argument".into(),
                })
            }
        }

        "cos" => {
            if let Some(arg) = args.first() {
                let rad = match arg {
                    Value::Float(f) => *f,
                    Value::Measurement(m) if m.dimension == UnitDimension::Angle => {
                        (m.raw as f64 / 1_000_000.0).to_radians()
                    }
                    Value::Int(i) => *i as f64,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Angle or Float for cos()",
                            found: other.type_name().to_string(),
                        })
                    }
                };
                Ok(Value::Float(rad.cos()))
            } else {
                Err(EvalError::General {
                    message: "cos() requires 1 argument".into(),
                })
            }
        }

        "tan" => {
            if let Some(arg) = args.first() {
                let rad = match arg {
                    Value::Float(f) => *f,
                    Value::Measurement(m) if m.dimension == UnitDimension::Angle => {
                        (m.raw as f64 / 1_000_000.0).to_radians()
                    }
                    Value::Int(i) => *i as f64,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            expected: "Angle or Float for tan()",
                            found: other.type_name().to_string(),
                        })
                    }
                };
                Ok(Value::Float(rad.tan()))
            } else {
                Err(EvalError::General {
                    message: "tan() requires 1 argument".into(),
                })
            }
        }

        "rect_between" => {
            if args.len() < 3 {
                return Err(EvalError::General {
                    message: "rect_between requires 3 arguments (p1: Point2D, p2: Point2D, width: Measurement)".into(),
                });
            }
            let p1 = args[0].coerce_to_point2d()?;
            let p2 = args[1].coerce_to_point2d()?;

            let (x1, y1) = match p1 {
                Value::Point2D { x, y } => (x, y),
                _ => unreachable!(),
            };
            let (x2, y2) = match p2 {
                Value::Point2D { x, y } => (x, y),
                _ => unreachable!(),
            };

            let half_w = match &args[2] {
                Value::Measurement(m) if m.dimension == UnitDimension::Length => {
                    (m.raw / 2) as i64
                }
                Value::Int(i) => *i / 2,
                other => {
                    return Err(EvalError::TypeMismatch {
                        expected: "Length measurement for rect_between width",
                        found: other.type_name().to_string(),
                    })
                }
            };

            let dx = (x2 - x1) as f64;
            let dy = (y2 - y1) as f64;
            let len = (dx * dx + dy * dy).sqrt();

            if len == 0.0 {
                return Ok(Value::Array(std::sync::Arc::new(vec![
                    Value::Point2D {
                        x: x1 - half_w,
                        y: y1 - half_w,
                    },
                    Value::Point2D {
                        x: x1 + half_w,
                        y: y1 - half_w,
                    },
                    Value::Point2D {
                        x: x1 + half_w,
                        y: y1 + half_w,
                    },
                    Value::Point2D {
                        x: x1 - half_w,
                        y: y1 + half_w,
                    },
                ])));
            }

            let nx = (-dy / len * half_w as f64).round() as i64;
            let ny = (dx / len * half_w as f64).round() as i64;

            Ok(Value::Array(std::sync::Arc::new(vec![
                Value::Point2D {
                    x: x1 + nx,
                    y: y1 + ny,
                },
                Value::Point2D {
                    x: x2 + nx,
                    y: y2 + ny,
                },
                Value::Point2D {
                    x: x2 - nx,
                    y: y2 - ny,
                },
                Value::Point2D {
                    x: x1 - nx,
                    y: y1 - ny,
                },
            ])))
        }

        _ => Err(EvalError::General {
            message: format!("Unknown built-in function '{}'", name),
        }),
    }
}
