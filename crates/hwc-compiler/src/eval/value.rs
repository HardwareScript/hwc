//! HardwareScript v0.3.0 Unified `Value` Model & 128-Bit Picometer Arithmetic
//!
//! Implements the canonical `Value` type, the `UnitDimension` taxonomy, and the
//! strict 128-bit integer dimensional algebra linked directly with `hwc_types::UnitRegistry`
//! and `hwc_parser::ast::Unit`.

use compact_str::CompactString;
use hwc_parser::ast::Unit;
use hwc_types::{NetId, UnitRegistry};
use std::sync::Arc;

use super::context::EvalError;

/// Canonical base units for dimensional checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitDimension {
    Length,       // pm (picometers, 10^-12 m)
    Voltage,      // nV (nanovolts, 10^-9 V)
    Current,      // pA (picoamperes, 10^-12 A)
    Resistance,   // uOhm (micro-ohms, 10^-6 Ohm)
    Capacitance,  // aF (attofarads, 10^-18 F)
    Inductance,   // pH (picohenries, 10^-12 H)
    Time,         // fs (femtoseconds, 10^-15 s)
    Frequency,    // Hz
    Power,        // pW (picowatts, 10^-12 W)
    Angle,        // micro-degrees (10^-6 deg)
    Temperature,  // mK (millikelvin)
    Conductivity, // S/m
    Resistivity,  // ohm-m
    Area,         // pm^2
    Dimensionless,
}

impl UnitDimension {
    /// Canonical conversion multiplier from SI base unit (meter, volt, ampere, ohm, etc.)
    /// to our internal 128-bit integer representation.
    pub const fn si_to_internal_scale(&self) -> f64 {
        match self {
            Self::Length => 1_000_000_000_000.0,      // 1 m = 10^12 pm
            Self::Voltage => 1_000_000_000.0,         // 1 V = 10^9 nV
            Self::Current => 1_000_000_000_000.0,     // 1 A = 10^12 pA
            Self::Resistance => 1_000_000.0,          // 1 Ohm = 10^6 uOhm
            Self::Capacitance => 1_000_000_000_000_000_000.0, // 1 F = 10^18 aF
            Self::Inductance => 1_000_000_000_000.0,  // 1 H = 10^12 pH
            Self::Time => 1_000_000_000_000_000.0,    // 1 s = 10^15 fs
            Self::Frequency => 1.0,                   // 1 Hz = 1 Hz
            Self::Power => 1_000_000_000_000.0,       // 1 W = 10^12 pW
            Self::Angle => 1_000_000.0,               // 1 deg = 10^6 micro-degrees
            Self::Temperature => 1_000.0,             // 1 K = 10^3 mK
            Self::Conductivity => 1.0,
            Self::Resistivity => 1.0,
            Self::Area => 1_000_000_000_000_000_000_000_000.0, // 1 m^2 = 10^24 pm^2
            Self::Dimensionless => 1.0,
        }
    }

    /// Parse a dimension string name into `UnitDimension`.
    pub fn from_dimension_str(dim: &str) -> Option<Self> {
        match dim.to_ascii_lowercase().as_str() {
            "length" | "distance" => Some(Self::Length),
            "voltage" => Some(Self::Voltage),
            "current" => Some(Self::Current),
            "resistance" => Some(Self::Resistance),
            "capacitance" => Some(Self::Capacitance),
            "inductance" => Some(Self::Inductance),
            "time" => Some(Self::Time),
            "frequency" => Some(Self::Frequency),
            "power" => Some(Self::Power),
            "angle" => Some(Self::Angle),
            "temperature" => Some(Self::Temperature),
            "conductivity" => Some(Self::Conductivity),
            "resistivity" => Some(Self::Resistivity),
            "area" => Some(Self::Area),
            "dimensionless" | "ratio" => Some(Self::Dimensionless),
            _ => None,
        }
    }
}

/// A physical measurement scaled to its dimension's canonical internal unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeasurementValue {
    /// 128-bit signed integer value scaled to the dimension's canonical internal unit.
    pub raw: i128,
    pub dimension: UnitDimension,
}

// Backward compatibility alias for existing code
pub type PhysicalValue = MeasurementValue;
pub type PhysicalDimension = UnitDimension;

impl MeasurementValue {
    /// Backward-compatibility accessor for `raw_value`
    #[inline]
    pub const fn raw_value(&self) -> i128 {
        self.raw
    }

    pub const fn new(raw: i128, dimension: UnitDimension) -> Self {
        Self { raw, dimension }
    }

    pub const fn length_pm(pm: i128) -> Self {
        Self::new(pm, UnitDimension::Length)
    }

    pub const fn voltage_nv(nv: i128) -> Self {
        Self::new(nv, UnitDimension::Voltage)
    }

    pub const fn current_pa(pa: i128) -> Self {
        Self::new(pa, UnitDimension::Current)
    }

    /// Convert from AST `Unit` and value using optional `UnitRegistry` for custom units.
    pub fn from_ast_unit(val: f64, unit: &Unit, registry: Option<&UnitRegistry>) -> Option<Self> {
        match unit {
            Unit::Distance(d) => {
                let pm = d.to_picometers(val);
                Some(Self::length_pm(pm as i128))
            }
            Unit::Voltage(v) => {
                let multiplier = match v {
                    hwc_parser::lexer::units::VoltageUnit::Volts => 1.0,
                    hwc_parser::lexer::units::VoltageUnit::Millivolts => 1e-3,
                    hwc_parser::lexer::units::VoltageUnit::Kilovolts => 1e3,
                };
                let base_v = val * multiplier;
                let nv = (base_v * UnitDimension::Voltage.si_to_internal_scale()).round() as i128;
                Some(Self::new(nv, UnitDimension::Voltage))
            }
            Unit::Current(c) => {
                let multiplier = match c {
                    hwc_parser::lexer::units::CurrentUnit::Amperes => 1.0,
                    hwc_parser::lexer::units::CurrentUnit::Milliamperes => 1e-3,
                    hwc_parser::lexer::units::CurrentUnit::Microamperes => 1e-6,
                };
                let base_a = val * multiplier;
                let pa = (base_a * UnitDimension::Current.si_to_internal_scale()).round() as i128;
                Some(Self::new(pa, UnitDimension::Current))
            }
            Unit::Temperature(_) => {
                let mk = (val * 1000.0).round() as i128;
                Some(Self::new(mk, UnitDimension::Temperature))
            }
            Unit::Custom(s) => Self::from_unit_str(val, s, registry),
        }
    }

    /// Convert a numeric value and unit string using `UnitRegistry` or canonical SI fallback lookup.
    pub fn from_unit_str(val: f64, unit_str: &str, registry: Option<&UnitRegistry>) -> Option<Self> {
        // 1. If registry is provided, use it as the single source of truth
        if let Some(reg) = registry {
            if let Some(info) = reg.get(unit_str) {
                if let Some(dim) = UnitDimension::from_dimension_str(info.dimension.as_str()) {
                    if let Some(multiplier) = info.multiplier {
                        let base_si = val * multiplier;
                        let raw = (base_si * dim.si_to_internal_scale()).round() as i128;
                        return Some(Self::new(raw, dim));
                    }
                }
            }
        }

        // 2. Direct lookup for standard units if registry not yet loaded or for standalone evaluation
        let (dim, multiplier) = match unit_str {
            // Distance
            "pm" => (UnitDimension::Length, 1e-12),
            "nm" => (UnitDimension::Length, 1e-9),
            "um" | "µm" => (UnitDimension::Length, 1e-6),
            "mm" => (UnitDimension::Length, 1e-3),
            "cm" => (UnitDimension::Length, 1e-2),
            "m" => (UnitDimension::Length, 1.0),
            "mil" => (UnitDimension::Length, 0.0000254),
            "in" => (UnitDimension::Length, 0.0254),

            // Voltage
            "nV" => (UnitDimension::Voltage, 1e-9),
            "uV" | "µV" => (UnitDimension::Voltage, 1e-6),
            "mV" => (UnitDimension::Voltage, 1e-3),
            "V" => (UnitDimension::Voltage, 1.0),
            "kV" => (UnitDimension::Voltage, 1e3),

            // Current
            "pA" => (UnitDimension::Current, 1e-12),
            "nA" => (UnitDimension::Current, 1e-9),
            "uA" | "µA" => (UnitDimension::Current, 1e-6),
            "mA" => (UnitDimension::Current, 1e-3),
            "A" => (UnitDimension::Current, 1.0),

            // Resistance
            "uOhm" | "µΩ" | "uohm" => (UnitDimension::Resistance, 1e-6),
            "mOhm" | "mΩ" | "mohm" => (UnitDimension::Resistance, 1e-3),
            "Ohm" | "Ω" | "ohm" => (UnitDimension::Resistance, 1.0),
            "kOhm" | "kΩ" | "kohm" => (UnitDimension::Resistance, 1e3),
            "MOhm" | "MΩ" | "megohm" => (UnitDimension::Resistance, 1e6),

            // Capacitance
            "aF" => (UnitDimension::Capacitance, 1e-18),
            "fF" => (UnitDimension::Capacitance, 1e-15),
            "pF" => (UnitDimension::Capacitance, 1e-12),
            "nF" => (UnitDimension::Capacitance, 1e-9),
            "uF" | "µF" => (UnitDimension::Capacitance, 1e-6),
            "mF" => (UnitDimension::Capacitance, 1e-3),
            "F" => (UnitDimension::Capacitance, 1.0),

            // Inductance
            "pH" => (UnitDimension::Inductance, 1e-12),
            "nH" => (UnitDimension::Inductance, 1e-9),
            "uH" | "µH" => (UnitDimension::Inductance, 1e-6),
            "mH" => (UnitDimension::Inductance, 1e-3),
            "H" => (UnitDimension::Inductance, 1.0),

            // Time
            "fs" => (UnitDimension::Time, 1e-15),
            "ps" => (UnitDimension::Time, 1e-12),
            "ns" => (UnitDimension::Time, 1e-9),
            "us" | "µs" => (UnitDimension::Time, 1e-6),
            "ms" => (UnitDimension::Time, 1e-3),
            "s" => (UnitDimension::Time, 1.0),

            // Frequency
            "Hz" => (UnitDimension::Frequency, 1.0),
            "kHz" => (UnitDimension::Frequency, 1e3),
            "MHz" => (UnitDimension::Frequency, 1e6),
            "GHz" => (UnitDimension::Frequency, 1e9),

            // Angle
            "deg" | "°" => (UnitDimension::Angle, 1.0),
            "rad" => (UnitDimension::Angle, 180.0 / std::f64::consts::PI),

            // Power
            "pW" => (UnitDimension::Power, 1e-12),
            "nW" => (UnitDimension::Power, 1e-9),
            "uW" | "µW" => (UnitDimension::Power, 1e-6),
            "mW" => (UnitDimension::Power, 1e-3),
            "W" => (UnitDimension::Power, 1.0),
            "kW" => (UnitDimension::Power, 1e3),

            _ => return None,
        };

        let base_si = val * multiplier;
        let raw = (base_si * dim.si_to_internal_scale()).round() as i128;
        Some(Self::new(raw, dim))
    }

    /// Backwards compatibility helper
    pub fn from_f64_unit(val: f64, unit_str: &str) -> Option<Self> {
        Self::from_unit_str(val, unit_str, None)
    }

    pub fn add(self, rhs: Self) -> Result<Self, EvalError> {
        if self.dimension != rhs.dimension {
            return Err(EvalError::UnitMismatch {
                expected: self.dimension,
                found: rhs.dimension,
                op: "+",
            });
        }
        Ok(Self {
            raw: self.raw + rhs.raw,
            dimension: self.dimension,
        })
    }

    pub fn sub(self, rhs: Self) -> Result<Self, EvalError> {
        if self.dimension != rhs.dimension {
            return Err(EvalError::UnitMismatch {
                expected: self.dimension,
                found: rhs.dimension,
                op: "-",
            });
        }
        Ok(Self {
            raw: self.raw - rhs.raw,
            dimension: self.dimension,
        })
    }

    pub fn mul_scalar(self, scalar: f64) -> Self {
        Self {
            raw: (self.raw as f64 * scalar).round() as i128,
            dimension: self.dimension,
        }
    }

    pub fn mul_measurement(self, rhs: Self) -> Result<Value, EvalError> {
        use UnitDimension::*;
        match (self.dimension, rhs.dimension) {
            // Length * Length -> Area (pm^2)
            (Length, Length) => Ok(Value::Measurement(Self {
                raw: self.raw * rhs.raw,
                dimension: Area,
            })),
            // Voltage * Current -> Power
            // (1 nV * 1 pA = 10^-9 V * 10^-12 A = 10^-21 W = 10^-9 pW)
            (Voltage, Current) | (Current, Voltage) => Ok(Value::Measurement(Self {
                raw: (self.raw * rhs.raw) / 1_000_000_000,
                dimension: Power,
            })),
            // Current * Resistance -> Voltage
            // (1 pA * 1 uOhm = 10^-12 A * 10^-6 Ohm = 10^-18 V = 10^-9 nV)
            (Current, Resistance) | (Resistance, Current) => Ok(Value::Measurement(Self {
                raw: (self.raw * rhs.raw) / 1_000_000_000,
                dimension: Voltage,
            })),
            _ => Err(EvalError::InvalidDimensionalMultiplication(
                self.dimension,
                rhs.dimension,
            )),
        }
    }

    pub fn div_measurement(self, rhs: Self) -> Result<Value, EvalError> {
        if rhs.raw == 0 {
            return Err(EvalError::DivisionByZero);
        }
        use UnitDimension::*;
        if self.dimension == rhs.dimension {
            return Ok(Value::Float(self.raw as f64 / rhs.raw as f64));
        }
        match (self.dimension, rhs.dimension) {
            // Voltage / Current -> Resistance
            (Voltage, Current) => Ok(Value::Measurement(Self {
                raw: (self.raw * 1_000_000_000) / rhs.raw,
                dimension: Resistance,
            })),
            // Area / Length -> Length
            (Area, Length) => Ok(Value::Measurement(Self {
                raw: self.raw / rhs.raw,
                dimension: Length,
            })),
            _ => Err(EvalError::InvalidDimensionalDivision(
                self.dimension,
                rhs.dimension,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

/// Unified value produced and manipulated by the compile-time evaluation engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // ── Primitive Literals ──
    Void,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(CompactString),

    // ── Physical & Geometric Primitives (Evaluated in Picometers) ──
    Measurement(MeasurementValue),
    Point2D {
        x: i64,
        y: i64,
    },
    Point3D {
        x: i64,
        y: i64,
        z: i64,
    },
    Vector2D {
        dx: i64,
        dy: i64,
    },
    BoundingBox {
        min_x: i64,
        min_y: i64,
        max_x: i64,
        max_y: i64,
    },

    // ── Composites & References ──
    Array(Arc<Vec<Value>>),
    StructInstance {
        name: CompactString,
        fields: Arc<Vec<(CompactString, Value)>>,
    },
    EnumVariant {
        enum_name: CompactString,
        variant_name: CompactString,
        payload: Option<Arc<Vec<Value>>>,
    },
    EnumType {
        name: CompactString,
        variants: Arc<rustc_hash::FxHashMap<CompactString, Value>>,
    },
    FunctionRef(FunctionId),

    // ── Hardware Domain Handles ──
    NetHandle(NetId),
    SpaceHandle(SpaceId),
    DeviceHandle(DeviceId),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Void => "Void",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Measurement(m) => match m.dimension {
                UnitDimension::Length => "Length",
                UnitDimension::Voltage => "Voltage",
                UnitDimension::Current => "Current",
                UnitDimension::Resistance => "Resistance",
                UnitDimension::Capacitance => "Capacitance",
                UnitDimension::Inductance => "Inductance",
                UnitDimension::Time => "Time",
                UnitDimension::Frequency => "Frequency",
                UnitDimension::Power => "Power",
                UnitDimension::Angle => "Angle",
                UnitDimension::Temperature => "Temperature",
                UnitDimension::Area => "Area",
                _ => "Measurement",
            },
            Value::Point2D { .. } => "Point2D",
            Value::Point3D { .. } => "Point3D",
            Value::Vector2D { .. } => "Vector2D",
            Value::BoundingBox { .. } => "BoundingBox",
            Value::Array(_) => "Array",
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
            Value::StructInstance { name, fields } if name.as_str() == "Point2D" => {
                // Extract x and y fields from the Point2D struct
                let x_val = fields.iter()
                    .find(|(k, _)| k.as_str() == "x")
                    .map(|(_, v)| v)
                    .ok_or_else(|| EvalError::General {
                        message: "Point2D struct missing 'x' field".to_string(),
                    })?;
                let y_val = fields.iter()
                    .find(|(k, _)| k.as_str() == "y")
                    .map(|(_, v)| v)
                    .ok_or_else(|| EvalError::General {
                        message: "Point2D struct missing 'y' field".to_string(),
                    })?;
                
                match (x_val, y_val) {
                    (Value::Measurement(mx), Value::Measurement(my)) 
                        if mx.dimension == UnitDimension::Length && my.dimension == UnitDimension::Length =>
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
                        expected: "Point2D with Length measurements",
                        found: format!("Point2D {{ x: {:?}, y: {:?} }}", x_val, y_val),
                        hint: "Point2D fields must be Length measurements",
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
                        if m.dimension == UnitDimension::Length
                            && n.dimension == UnitDimension::Length =>
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
        eprintln!("[VALUE DEBUG] Division: {:?} / {:?}", self.type_name(), other.type_name());
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
                eprintln!("[VALUE DEBUG] Division failed - detailed: {:?} / {:?}", a, b);
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

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Void => write!(f, "()"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "{}", s),
            Value::Measurement(m) => {
                // Human-readable engineering unit formatting
                match m.dimension {
                    UnitDimension::Length => {
                        let pm = m.raw;
                        if pm.abs() >= 1_000_000_000_000 {
                            write!(f, "{:.2}m", pm as f64 / 1e12)
                        } else if pm.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}mm", pm as f64 / 1e9)
                        } else if pm.abs() >= 1_000_000 {
                            write!(f, "{:.2}um", pm as f64 / 1e6)
                        } else if pm.abs() >= 1_000 {
                            write!(f, "{:.2}nm", pm as f64 / 1e3)
                        } else {
                            write!(f, "{}pm", pm)
                        }
                    }
                    UnitDimension::Voltage => {
                        let nv = m.raw;
                        if nv.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}kV", nv as f64 / 1e12)
                        } else if nv.abs() >= 1_000_000 {
                            write!(f, "{:.2}V", nv as f64 / 1e9)
                        } else if nv.abs() >= 1_000 {
                            write!(f, "{:.2}mV", nv as f64 / 1e6)
                        } else {
                            write!(f, "{:.2}uV", nv as f64 / 1e3)
                        }
                    }
                    UnitDimension::Current => {
                        let pa = m.raw;
                        if pa.abs() >= 1_000_000_000_000 {
                            write!(f, "{:.2}A", pa as f64 / 1e12)
                        } else if pa.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}mA", pa as f64 / 1e9)
                        } else if pa.abs() >= 1_000_000 {
                            write!(f, "{:.2}uA", pa as f64 / 1e6)
                        } else if pa.abs() >= 1_000 {
                            write!(f, "{:.2}nA", pa as f64 / 1e3)
                        } else {
                            write!(f, "{}pA", pa)
                        }
                    }
                    UnitDimension::Resistance => {
                        let uohm = m.raw;
                        if uohm.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}MOhm", uohm as f64 / 1e9)
                        } else if uohm.abs() >= 1_000_000 {
                            write!(f, "{:.2}kOhm", uohm as f64 / 1e6)
                        } else if uohm.abs() >= 1_000 {
                            write!(f, "{:.2}Ohm", uohm as f64 / 1e3)
                        } else {
                            write!(f, "{:.2}mOhm", uohm as f64 / 1.0)
                        }
                    }
                    UnitDimension::Capacitance => {
                        let af = m.raw;
                        if af.abs() >= 1_000_000_000_000 {
                            write!(f, "{:.2}uF", af as f64 / 1e18)
                        } else if af.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}nF", af as f64 / 1e15)
                        } else if af.abs() >= 1_000_000 {
                            write!(f, "{:.2}pF", af as f64 / 1e12)
                        } else if af.abs() >= 1_000 {
                            write!(f, "{:.2}fF", af as f64 / 1e3)
                        } else {
                            write!(f, "{}aF", af)
                        }
                    }
                    UnitDimension::Inductance => {
                        let ph = m.raw;
                        if ph.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}mH", ph as f64 / 1e12)
                        } else if ph.abs() >= 1_000_000 {
                            write!(f, "{:.2}uH", ph as f64 / 1e9)
                        } else if ph.abs() >= 1_000 {
                            write!(f, "{:.2}nH", ph as f64 / 1e6)
                        } else {
                            write!(f, "{}pH", ph)
                        }
                    }
                    UnitDimension::Time => {
                        let fs = m.raw;
                        if fs.abs() >= 1_000_000_000_000 {
                            write!(f, "{:.2}s", fs as f64 / 1e15)
                        } else if fs.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}ms", fs as f64 / 1e12)
                        } else if fs.abs() >= 1_000_000 {
                            write!(f, "{:.2}us", fs as f64 / 1e9)
                        } else if fs.abs() >= 1_000 {
                            write!(f, "{:.2}ns", fs as f64 / 1e6)
                        } else {
                            write!(f, "{}fs", fs)
                        }
                    }
                    UnitDimension::Frequency => {
                        let hz = m.raw as f64;
                        if hz.abs() >= 1e9 {
                            write!(f, "{:.2}GHz", hz / 1e9)
                        } else if hz.abs() >= 1e6 {
                            write!(f, "{:.2}MHz", hz / 1e6)
                        } else if hz.abs() >= 1e3 {
                            write!(f, "{:.2}kHz", hz / 1e3)
                        } else {
                            write!(f, "{:.2}Hz", hz)
                        }
                    }
                    UnitDimension::Power => {
                        let pw = m.raw;
                        if pw.abs() >= 1_000_000_000 {
                            write!(f, "{:.2}mW", pw as f64 / 1e12)
                        } else if pw.abs() >= 1_000_000 {
                            write!(f, "{:.2}uW", pw as f64 / 1e9)
                        } else if pw.abs() >= 1_000 {
                            write!(f, "{:.2}nW", pw as f64 / 1e6)
                        } else {
                            write!(f, "{}pW", pw)
                        }
                    }
                    UnitDimension::Angle => {
                        let udeg = m.raw;
                        write!(f, "{:.2}deg", udeg as f64 / 1e6)
                    }
                    UnitDimension::Temperature => {
                        let mk = m.raw;
                        write!(f, "{:.2}K", mk as f64 / 1e3)
                    }
                    UnitDimension::Area => {
                        let pm2 = m.raw;
                        if pm2.abs() >= 1_000_000_000_000_000_000_000_000 {
                            write!(f, "{:.2}m^2", pm2 as f64 / 1e24)
                        } else if pm2.abs() >= 1_000_000_000_000_000_000 {
                            write!(f, "{:.2}mm^2", pm2 as f64 / 1e18)
                        } else if pm2.abs() >= 1_000_000_000_000 {
                            write!(f, "{:.2}um^2", pm2 as f64 / 1e12)
                        } else if pm2.abs() >= 1_000_000 {
                            write!(f, "{:.2}nm^2", pm2 as f64 / 1e6)
                        } else {
                            write!(f, "{}pm^2", pm2)
                        }
                    }
                    _ => write!(f, "{:?}({})", m.dimension, m.raw),
                }
            }
            Value::Point2D { x, y } => write!(f, "Point2D[{}, {}]", x, y),
            Value::Point3D { x, y, z } => write!(f, "Point3D[{}, {}, {}]", x, y, z),
            Value::Vector2D { dx, dy } => write!(f, "Vector2D[{}, {}]", dx, dy),
            Value::BoundingBox {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                write!(
                    f,
                    "BoundingBox[{}, {}, {}, {}]",
                    min_x, min_y, max_x, max_y
                )
            }
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::StructInstance { name, fields } => {
                write!(f, "{} {{ ", name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                payload,
            } => match payload {
                Some(p) => write!(f, "{}::{}({:?})", enum_name, variant_name, p),
                None => write!(f, "{}::{}", enum_name, variant_name),
            },
            Value::EnumType { name, .. } => write!(f, "<enum {}>", name),
            Value::FunctionRef(id) => write!(f, "<fn {:?}>", id),
            Value::NetHandle(id) => write!(f, "<net #{}>", id.0),
            Value::SpaceHandle(id) => write!(f, "<space #{}>", id.0),
            Value::DeviceHandle(id) => write!(f, "<device #{}>", id.0),
        }
    }
}
