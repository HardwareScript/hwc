//! HardwareScript v0.3.0 Standard Built-In Functions

use super::context::EvalError;
use super::value::{MeasurementValue, UnitDimension, Value};

/// Check if a name is a standard built-in function
pub fn is_builtin(name: &str) -> bool {
    get_builtin_id(name).is_some()
}

/// Map function name to standard builtin ID
pub fn get_builtin_id(name: &str) -> Option<u8> {
    match name {
        "println" => Some(0x01),
        "eprintln" => Some(0x02),
        "dbg" => Some(0x03),
        "assert" => Some(0x04),
        "min" => Some(0x05),
        "max" => Some(0x06),
        "abs" => Some(0x07),
        "sqrt" => Some(0x08),
        "sin" => Some(0x09),
        "cos" => Some(0x0A),
        "tan" => Some(0x0B),
        "rect_between" => Some(0x0C),
        "range" => Some(0x0D),
        "int" => Some(0x0E),
        "float" => Some(0x0F),
        "bbox_intersects" => Some(0x10),
        "bbox_union" => Some(0x11),
        "bbox_from_rect" => Some(0x12),
        _ => None,
    }
}

/// Dispatch builtin by ID
pub fn dispatch_builtin(id: u8, args: Vec<Value>) -> Result<Value, EvalError> {
    match id {
        0x01 => call_builtin("println", args),
        0x02 => call_builtin("eprintln", args),
        0x03 => call_builtin("dbg", args),
        0x04 => call_builtin("assert", args),
        0x05 => call_builtin("min", args),
        0x06 => call_builtin("max", args),
        0x07 => call_builtin("abs", args),
        0x08 => call_builtin("sqrt", args),
        0x09 => call_builtin("sin", args),
        0x0A => call_builtin("cos", args),
        0x0B => call_builtin("tan", args),
        0x0C => call_builtin("rect_between", args),
        0x0D => {
            // Range builtin: args[0] = start, args[1] = end, args[2] = inclusive
            let start = match args.first() {
                Some(Value::Int(i)) => *i,
                _ => 0,
            };
            let end = match args.get(1) {
                Some(Value::Int(i)) => *i,
                _ => 0,
            };
            let inclusive = match args.get(2) {
                Some(Value::Bool(b)) => *b,
                _ => false,
            };
            let range_vec: Vec<Value> = if inclusive {
                (start..=end).map(Value::Int).collect()
            } else {
                (start..end).map(Value::Int).collect()
            };
            Ok(Value::Array(std::sync::Arc::new(range_vec)))
        }
        0x0E => call_builtin("int", args),
        0x0F => call_builtin("float", args),
        0x10 => call_builtin("bbox_intersects", args),
        0x11 => call_builtin("bbox_union", args),
        0x12 => call_builtin("bbox_from_rect", args),
        _ => Err(EvalError::General {
            message: format!("Unknown builtin id 0x{:02X}", id),
        }),
    }
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
            if let Some(first) = args.first() {
                if let Value::String(fmt_str) = first {
                    let mut rendered = fmt_str.to_string();
                    for (i, arg) in args.iter().skip(1).enumerate() {
                        let placeholder = format!("{{{}}}", i);
                        rendered = rendered.replace(&placeholder, &format!("{}", arg));
                    }
                    eprintln!("{}", rendered);
                } else {
                    let items: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                    eprintln!("{}", items.join(" "));
                }
            } else {
                eprintln!();
            }
            Ok(Value::Void)
        }

        "dbg" => {
            if let Some(val) = args.first() {
                eprintln!("[DBG] {:?}", val);
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
                    Value::Measurement(m) if m.dimension == UnitDimension::ANGLE => {
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
                    Value::Measurement(m) if m.dimension == UnitDimension::ANGLE => {
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
                    Value::Measurement(m) if m.dimension == UnitDimension::ANGLE => {
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
                Value::Measurement(m) if m.dimension == UnitDimension::LENGTH => {
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

        "int" => {
            if let Some(arg) = args.first() {
                match arg {
                    Value::Int(i) => Ok(Value::Int(*i)),
                    Value::Float(f) => Ok(Value::Int(*f as i64)),
                    Value::String(s) => s.parse::<i64>().map(Value::Int).map_err(|_| {
                        EvalError::General { message: format!("Cannot parse '{}' as Int", s) }
                    }),
                    Value::Measurement(m) => Ok(Value::Int(m.raw as i64)),
                    other => Err(EvalError::TypeMismatch {
                        expected: "Float, Int, String or Measurement for int()",
                        found: other.type_name().to_string(),
                    }),
                }
            } else {
                Err(EvalError::General { message: "int() requires 1 argument".into() })
            }
        }

        "float" => {
            if let Some(arg) = args.first() {
                match arg {
                    Value::Float(f) => Ok(Value::Float(*f)),
                    Value::Int(i) => Ok(Value::Float(*i as f64)),
                    Value::String(s) => s.parse::<f64>().map(Value::Float).map_err(|_| {
                        EvalError::General { message: format!("Cannot parse '{}' as Float", s) }
                    }),
                    Value::Measurement(m) => {
                        let scale = m.dimension.si_to_internal_scale();
                        Ok(Value::Float(m.raw as f64 / scale))
                    }
                    other => Err(EvalError::TypeMismatch {
                        expected: "Int, Float, String or Measurement for float()",
                        found: other.type_name().to_string(),
                    }),
                }
            } else {
                Err(EvalError::General { message: "float() requires 1 argument".into() })
            }
        }

        "bbox_intersects" => {
            if args.len() < 2 {
                return Err(EvalError::General { message: "bbox_intersects requires 2 arguments".into() });
            }
            let (min_x1, min_y1, max_x1, max_y1) = extract_bbox(&args[0])?;
            let (min_x2, min_y2, max_x2, max_y2) = extract_bbox(&args[1])?;
            let overlaps = !(max_x1 < min_x2 || min_x1 > max_x2 || max_y1 < min_y2 || min_y1 > max_y2);
            Ok(Value::Bool(overlaps))
        }

        "bbox_union" => {
            if args.len() < 2 {
                return Err(EvalError::General { message: "bbox_union requires 2 arguments".into() });
            }
            let (min_x1, min_y1, max_x1, max_y1) = extract_bbox(&args[0])?;
            let (min_x2, min_y2, max_x2, max_y2) = extract_bbox(&args[1])?;
            Ok(Value::BoundingBox {
                min_x: min_x1.min(min_x2),
                min_y: min_y1.min(min_y2),
                max_x: max_x1.max(max_x2),
                max_y: max_y1.max(max_y2),
            })
        }

        "bbox_from_rect" => {
            // Can be called with (center: Point2D, size: [w, h]) or (x, y, w, h)
            if args.len() == 2 {
                let center_pt = args[0].coerce_to_point2d()?;
                let (cx, cy) = match center_pt {
                    Value::Point2D { x, y } => (x, y),
                    _ => unreachable!(),
                };
                let (w, h) = match &args[1] {
                    Value::Array(items) if items.len() >= 2 => {
                        let w_pm = match &items[0] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                        let h_pm = match &items[1] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                        (w_pm, h_pm)
                    }
                    other => return Err(EvalError::TypeMismatch {
                        expected: "Array [w, h] for bbox_from_rect size",
                        found: other.type_name().to_string(),
                    }),
                };
                Ok(Value::BoundingBox {
                    min_x: cx - w / 2,
                    min_y: cy - h / 2,
                    max_x: cx + w / 2,
                    max_y: cy + h / 2,
                })
            } else if args.len() >= 4 {
                let x = match &args[0] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let y = match &args[1] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let w = match &args[2] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let h = match &args[3] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                Ok(Value::BoundingBox {
                    min_x: x,
                    min_y: y,
                    max_x: x + w,
                    max_y: y + h,
                })
            } else {
                Err(EvalError::General { message: "bbox_from_rect requires 2 or 4 arguments".into() })
            }
        }

        _ => Err(EvalError::General {
            message: format!("Unknown built-in function '{}'", name),
        }),
    }
}

fn extract_bbox(val: &Value) -> Result<(i64, i64, i64, i64), EvalError> {
    match val {
        Value::BoundingBox { min_x, min_y, max_x, max_y } => Ok((*min_x, *min_y, *max_x, *max_y)),
        Value::StructInstance { name, fields } if name.as_str() == "BoundingBox" => {
            let mut min_x = 0;
            let mut min_y = 0;
            let mut max_x = 0;
            let mut max_y = 0;
            for (k, v) in fields.iter() {
                let raw = match v {
                    Value::Measurement(m) => m.raw as i64,
                    Value::Int(i) => *i,
                    _ => 0,
                };
                match k.as_str() {
                    "min_x" => min_x = raw,
                    "min_y" => min_y = raw,
                    "max_x" => max_x = raw,
                    "max_y" => max_y = raw,
                    _ => {}
                }
            }
            Ok((min_x, min_y, max_x, max_y))
        }
        other => Err(EvalError::TypeMismatch {
            expected: "BoundingBox",
            found: other.type_name().to_string(),
        }),
    }
}
