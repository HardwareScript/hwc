use compact_str::CompactString;
use hwc_types::{NetId, SiDimension};
use super::super::context::EvalError;
use super::measurement::MeasurementValue;
use super::value_enum::Value;

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Void => "Void",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Measurement(m) => {
                if m.dimension == SiDimension::LENGTH {
                    "Length"
                } else if m.dimension == SiDimension::VOLTAGE {
                    "Voltage"
                } else if m.dimension == SiDimension::CURRENT {
                    "Current"
                } else if m.dimension == SiDimension::RESISTANCE {
                    "Resistance"
                } else if m.dimension == SiDimension::CAPACITANCE {
                    "Capacitance"
                } else if m.dimension == SiDimension::INDUCTANCE {
                    "Inductance"
                } else if m.dimension == SiDimension::TIME {
                    "Time"
                } else if m.dimension == SiDimension::FREQUENCY {
                    "Frequency"
                } else if m.dimension == SiDimension::POWER {
                    "Power"
                } else if m.dimension == SiDimension::ANGLE {
                    "Angle"
                } else if m.dimension == SiDimension::TEMPERATURE {
                    "Temperature"
                } else if m.dimension == SiDimension::AREA {
                    "Area"
                } else if m.dimension == SiDimension::VOLUME {
                    "Volume"
                } else if m.dimension == SiDimension::SHEET_RES {
                    "SheetResistance"
                } else if m.dimension == SiDimension::CAPACITANCE_DENSITY {
                    "CapacitanceDensity"
                } else {
                    "Measurement"
                }
            }
            Value::Point2D { .. } => "Point2D",
            Value::Point3D { .. } => "Point3D",
            Value::Vector2D { .. } => "Vector2D",
            Value::BoundingBox { .. } => "BoundingBox",
            Value::CellLayout(_) => "CellLayout",
            Value::PlacedCell(_) => "PlacedCell",
            Value::PlacedPort(_) => "PlacedPort",
            Value::Array(_) => "Array",
            Value::Tuple(_) => "Tuple",
            Value::StructInstance { .. } => "StructInstance",
            Value::EnumVariant { .. } => "EnumVariant",
            Value::EnumType { .. } => "EnumType",
            Value::FunctionRef(_) => "Function",
            Value::NetHandle(_) => "Net",
            Value::SpaceHandle(_) => "Space",
            Value::DeviceHandle(_) => "Device",
        }
    }

    /// Array-to-`Point2D` coercion: `[Measurement, Measurement]` -> `Point2D` in picometers.
    pub fn coerce_to_point2d(&self) -> Result<Value, EvalError> {
        match self {
            Value::Point2D { .. } => Ok(self.clone()),
            Value::PlacedPort(p) => Ok(Value::Point2D { x: p.world_x, y: p.world_y }),
            Value::StructInstance { fields, .. } if fields.iter().any(|(k, _)| k.as_str() == "x") && fields.iter().any(|(k, _)| k.as_str() == "y") => {
                // Extract x and y fields from the struct
                let x_val = fields.iter()
                    .find(|(k, _)| k.as_str() == "x")
                    .map(|(_, v)| v)
                    .ok_or_else(|| EvalError::General {
                        message: "Struct missing 'x' field".to_string(),
                    })?;
                let y_val = fields.iter()
                    .find(|(k, _)| k.as_str() == "y")
                    .map(|(_, v)| v)
                    .ok_or_else(|| EvalError::General {
                        message: "Struct missing 'y' field".to_string(),
                    })?;

                match (x_val, y_val) {
                    (Value::Measurement(mx), Value::Measurement(my))
                        if mx.dimension == SiDimension::LENGTH && my.dimension == SiDimension::LENGTH =>
                    {
                        Ok(Value::Point2D {
                            x: mx.raw as i64,
                            y: my.raw as i64,
                        })
                    }
                    (Value::Int(ix), Value::Int(iy)) => {
                        Ok(Value::Point2D {
                            x: *ix,
                            y: *iy,
                        })
                    }
                    _ => Err(EvalError::CoercionFailed {
                        expected: "Struct with Length measurements",
                        found: format!("Struct {{ x: {:?}, y: {:?} }}", x_val, y_val),
                        hint: "Fields must be Length measurements",
                    })
                }
            }
            Value::StructInstance { fields, .. } if fields.iter().any(|(k, _)| k.as_str() == "center") => {
                if let Some((_, center_val)) = fields.iter().find(|(k, _)| k.as_str() == "center") {
                    center_val.coerce_to_point2d()
                } else {
                    Err(EvalError::TypeMismatch {
                        expected: "Point2D or struct with 'center'",
                        found: format!("{:?}", self),
                    })
                }
            }
            Value::Array(items) => {
                if items.len() != 2 {
                    return Err(EvalError::CoercionFailed {
                        expected: "Point2D",
                        found: format!("Array of length {}", items.len()),
                        hint: "Point2D array literal coercion requires exactly [x: Measurement, y: Measurement]",
                    });
                }
                match (&items[0], &items[1]) {
                    (Value::Measurement(m), Value::Measurement(n))
                        if m.dimension == SiDimension::LENGTH
                            && n.dimension == SiDimension::LENGTH =>
                    {
                        Ok(Value::Point2D {
                            x: m.raw as i64,
                            y: n.raw as i64,
                        })
                    }
                    (Value::Int(m), Value::Int(n)) => {
                        Ok(Value::Point2D {
                            x: *m,
                            y: *n,
                        })
                    }
                    (a, b) => Err(EvalError::CoercionFailed {
                        expected: "Point2D (both Length measurements)",
                        found: format!("[{:?}, {:?}]", a, b),
                        hint: "Array elements must both be Length measurements (e.g., [10.0um, 5.0um])",
                    }),
                }
            }
            other => Err(EvalError::TypeMismatch {
                expected: "Point2D or [Measurement, Measurement]",
                found: format!("{:?}", other),
            }),
        }
    }

    /// Coerce value to expected type name
    pub fn coerce_to_type(&self, expected_type: &str) -> Result<Value, EvalError> {
        match expected_type {
            "Point2D" => self.coerce_to_point2d(),
            "String" => match self {
                Value::String(_) => Ok(self.clone()),
                other => Ok(Value::String(format!("{}", other).into())),
            },
            "Int" => match self {
                Value::Int(_) => Ok(self.clone()),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                other => Err(EvalError::TypeMismatch {
                    expected: "Int",
                    found: other.type_name().to_string(),
                }),
            },
            "Float" => match self {
                Value::Float(_) => Ok(self.clone()),
                Value::Int(i) => Ok(Value::Float(*i as f64)),
                other => Err(EvalError::TypeMismatch {
                    expected: "Float",
                    found: other.type_name().to_string(),
                }),
            },
            _ => Ok(self.clone()),
        }
    }

    pub fn as_compact_str(&self) -> Result<&CompactString, EvalError> {
        match self {
            Value::String(s) => Ok(s),
            other => Err(EvalError::TypeMismatch {
                expected: "String",
                found: other.type_name().to_string(),
            }),
        }
    }

    #[inline(always)]
    pub fn add_fast(lhs: &Value, rhs: &Value) -> Result<Value, String> {
        match (lhs, rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Measurement(a), Value::Measurement(b)) => {
                if a.dimension != b.dimension {
                    return Err(format!("Unit mismatch in addition: {:?} vs {:?}", a.dimension, b.dimension));
                }
                Ok(Value::Measurement(MeasurementValue {
                    raw: a.raw + b.raw,
                    dimension: a.dimension,
                }))
            }
            _ => lhs.add(rhs).map_err(|e| e.to_string()),
        }
    }

    pub fn to_points_pm(&self) -> Vec<(i64, i64)> {
        match self {
            Value::Array(items) => items.iter().filter_map(|v| match v {
                Value::Point2D { x, y } => Some((*x, *y)),
                _ => None,
            }).collect(),
            Value::BoundingBox { min_x, min_y, max_x, max_y } => {
                vec![
                    (*min_x, *min_y),
                    (*max_x, *min_y),
                    (*max_x, *max_y),
                    (*min_x, *max_y),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub fn as_net_id(&self) -> Option<u32> {
        match self {
            Value::NetHandle(id) => Some(id.0),
            _ => None,
        }
    }

    pub fn as_net_handle(&self) -> Result<NetId, EvalError> {
        match self {
            Value::NetHandle(id) => Ok(*id),
            other => Err(EvalError::TypeMismatch {
                expected: "NetHandle",
                found: other.type_name().to_string(),
            }),
        }
    }

    pub fn as_measurement_raw(&self) -> Result<i128, EvalError> {
        match self {
            Value::Measurement(m) => Ok(m.raw),
            other => Err(EvalError::TypeMismatch {
                expected: "Measurement",
                found: other.type_name().to_string(),
            }),
        }
    }

    pub fn as_struct_fields(&self) -> Result<&[(CompactString, Value)], EvalError> {
        match self {
            Value::StructInstance { fields, .. } => Ok(fields),
            other => Err(EvalError::TypeMismatch {
                expected: "StructInstance",
                found: other.type_name().to_string(),
            }),
        }
    }
}
