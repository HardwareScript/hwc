use super::super::context::EvalError;
use super::measurement::MeasurementValue;
use super::value_enum::Value;

impl Value {
    pub fn add(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
            (Value::String(a), Value::String(b)) => {
                Ok(Value::String(format!("{}{}", a, b).into()))
            }
            (Value::Measurement(a), Value::Measurement(b)) => {
                Ok(Value::Measurement(a.add(*b)?))
            }
            (Value::Point2D { x, y }, Value::Vector2D { dx, dy }) => Ok(Value::Point2D {
                x: x + dx,
                y: y + dy,
            }),
            (Value::Vector2D { dx, dy }, Value::Point2D { x, y }) => Ok(Value::Point2D {
                x: x + dx,
                y: y + dy,
            }),
            (
                Value::Vector2D {
                    dx: x1,
                    dy: y1,
                },
                Value::Vector2D {
                    dx: x2,
                    dy: y2,
                },
            ) => Ok(Value::Vector2D {
                dx: x1 + x2,
                dy: y1 + y2,
            }),
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Addable types",
                found: format!("{} + {}", a.type_name(), b.type_name()),
            }),
        }
    }

    pub fn sub(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
            (Value::Measurement(a), Value::Measurement(b)) => {
                Ok(Value::Measurement(a.sub(*b)?))
            }
            (Value::Point2D { x: x1, y: y1 }, Value::Point2D { x: x2, y: y2 }) => {
                Ok(Value::Vector2D {
                    dx: x1 - x2,
                    dy: y1 - y2,
                })
            }
            (Value::Point2D { x, y }, Value::Vector2D { dx, dy }) => Ok(Value::Point2D {
                x: x - dx,
                y: y - dy,
            }),
            (
                Value::Vector2D {
                    dx: x1,
                    dy: y1,
                },
                Value::Vector2D {
                    dx: x2,
                    dy: y2,
                },
            ) => Ok(Value::Vector2D {
                dx: x1 - x2,
                dy: y1 - y2,
            }),
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Subtractable types",
                found: format!("{} - {}", a.type_name(), b.type_name()),
            }),
        }
    }

    pub fn mul(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
            (Value::Measurement(m), Value::Int(n)) | (Value::Int(n), Value::Measurement(m)) => {
                Ok(Value::Measurement(m.mul_scalar(*n as f64)))
            }
            (Value::Measurement(m), Value::Float(n)) | (Value::Float(n), Value::Measurement(m)) => {
                Ok(Value::Measurement(m.mul_scalar(*n)))
            }
            (Value::Measurement(a), Value::Measurement(b)) => a.mul_measurement(*b),
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Multipliable types",
                found: format!("{} * {}", a.type_name(), b.type_name()),
            }),
        }
    }

    pub fn div(&self, other: &Value) -> Result<Value, EvalError> {
                match (self, other) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => {
                if *b == 0.0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Float(a / b))
            }
            (Value::Measurement(m), Value::Int(n)) => {
                if *n == 0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Measurement(m.mul_scalar(1.0 / *n as f64)))
            }
            (Value::Measurement(m), Value::Float(n)) => {
                if *n == 0.0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Measurement(m.mul_scalar(1.0 / n)))
            }
            (Value::Measurement(a), Value::Measurement(b)) => a.div_measurement(*b),
            (a, b) => {
                                Err(EvalError::TypeMismatch {
                    expected: "Dividable types",
                    found: format!("{} / {}", a.type_name(), b.type_name()),
                })
            }
        }
    }

    pub fn modulo(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Int(a % b))
            }
            _ => Err(EvalError::TypeMismatch {
                expected: "Int",
                found: format!("{} % {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn bitwise_and(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a & b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
            _ => Err(EvalError::TypeMismatch {
                expected: "Int or Bool for &",
                found: format!("{} & {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn bitwise_or(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a | b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
            _ => Err(EvalError::TypeMismatch {
                expected: "Int or Bool for |",
                found: format!("{} | {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn bitwise_xor(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a ^ b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a ^ *b)),
            _ => Err(EvalError::TypeMismatch {
                expected: "Int or Bool for ^",
                found: format!("{} ^ {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn bitwise_not(&self) -> Result<Value, EvalError> {
        match self {
            Value::Int(a) => Ok(Value::Int(!a)),
            Value::Bool(a) => Ok(Value::Bool(!a)),
            _ => Err(EvalError::TypeMismatch {
                expected: "Int or Bool for ~",
                found: self.type_name().to_string(),
            }),
        }
    }

    pub fn shift_left(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => {
                if *b < 0 || *b >= 64 {
                    Ok(Value::Int(0))
                } else {
                    Ok(Value::Int(a << b))
                }
            }
            _ => Err(EvalError::TypeMismatch {
                expected: "Int for <<",
                found: format!("{} << {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn shift_right(&self, other: &Value) -> Result<Value, EvalError> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => {
                if *b < 0 || *b >= 64 {
                    Ok(Value::Int(0))
                } else {
                    Ok(Value::Int(a >> b))
                }
            }
            _ => Err(EvalError::TypeMismatch {
                expected: "Int for >>",
                found: format!("{} >> {}", self.type_name(), other.type_name()),
            }),
        }
    }

    pub fn neg(&self) -> Result<Value, EvalError> {
        match self {
            Value::Int(i) => Ok(Value::Int(-i)),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::Measurement(m) => {
                Ok(Value::Measurement(MeasurementValue::new(-m.raw, m.dimension)))
            }
            other => Err(EvalError::TypeMismatch {
                expected: "Numeric",
                found: other.type_name().to_string(),
            }),
        }
    }
}
