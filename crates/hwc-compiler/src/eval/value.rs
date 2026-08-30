//! HardwareScript v0.3.1 Unified `Value` Model & 7-Base SI Picometer Dimensional Arithmetic
//!
//! Implements the canonical `Value` type, the 7-Base SI `SiDimension` vector,
//! pure `CellLayout` composition, and strict 128-bit integer dimensional algebra.

use compact_str::CompactString;
use hwc_parser::ast::Unit;
use hwc_types::{NetId, SiDimension, UnitRegistry};
use std::sync::Arc;

use super::context::EvalError;

pub type UnitDimension = SiDimension;
pub type PhysicalDimension = SiDimension;

/// Relative geometric polygon in a pure CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellPolygon {
    pub layer: CompactString,
    pub points: Vec<(i64, i64)>, // local coordinates in picometers
    pub net: Option<NetId>,
}

/// Relative contact cut / via pillar in a pure CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellContact {
    pub name: CompactString,
    pub from_layer: CompactString,
    pub to_layer: CompactString,
    pub at: (i64, i64), // local coordinates in picometers
    pub diameter: i64,  // diameter in picometers
    pub net: Option<NetId>,
}

/// Named connection port exposed by a CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellPort {
    pub name: CompactString,
    pub at: (i64, i64), // local coordinates in picometers
    pub layer: CompactString,
    pub net: Option<NetId>,
}

/// Local sub-device declaration inside a CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellDevice {
    pub device_type: CompactString,
    pub instance_name: CompactString,
    pub terminals: Vec<(CompactString, CompactString)>,
    pub params: Vec<(CompactString, Value)>,
}

/// 2D Transformation (Rotation, Mirror, Translation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Transform2D {
    pub rotation_deg: i32,
    pub mirror_x: bool,
    pub mirror_y: bool,
    pub offset_x: i64,
    pub offset_y: i64,
}

impl Transform2D {
    pub fn apply_point(&self, pt: (i64, i64)) -> (i64, i64) {
        let mut x = pt.0;
        let mut y = pt.1;

        if self.mirror_x {
            y = -y;
        }
        if self.mirror_y {
            x = -x;
        }

        let rot = ((self.rotation_deg % 360) + 360) % 360;
        let (rx, ry) = match rot {
            90 => (-y, x),
            180 => (-x, -y),
            270 => (y, -x),
            _ => {
                if rot != 0 {
                    let rad = (rot as f64) * std::f64::consts::PI / 180.0;
                    let cos_r = rad.cos();
                    let sin_r = rad.sin();
                    let nx = (x as f64 * cos_r - y as f64 * sin_r).round() as i64;
                    let ny = (x as f64 * sin_r + y as f64 * cos_r).round() as i64;
                    (nx, ny)
                } else {
                    (x, y)
                }
            }
        };

        (rx + self.offset_x, ry + self.offset_y)
    }
}

/// Pure, self-contained cell layout container (Pillar 1)
#[derive(Debug, Clone, PartialEq)]
pub struct CellLayout {
    pub name: CompactString,
    pub polygons: Vec<CellPolygon>,
    pub contacts: Vec<CellContact>,
    pub ports: Vec<CellPort>,
    pub devices: Vec<CellDevice>,
    pub transform: Transform2D,
}

impl CellLayout {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            polygons: Vec::new(),
            contacts: Vec::new(),
            ports: Vec::new(),
            devices: Vec::new(),
            transform: Transform2D::default(),
        }
    }

    pub fn rotate(&self, deg: i32) -> Self {
        let mut copy = self.clone();
        copy.transform.rotation_deg = ((copy.transform.rotation_deg + deg) % 360 + 360) % 360;
        copy
    }

    pub fn mirror_x(&self) -> Self {
        let mut copy = self.clone();
        copy.transform.mirror_x = !copy.transform.mirror_x;
        copy
    }

    pub fn mirror_y(&self) -> Self {
        let mut copy = self.clone();
        copy.transform.mirror_y = !copy.transform.mirror_y;
        copy
    }

    pub fn offset(&self, dx: i64, dy: i64) -> Self {
        let mut copy = self.clone();
        copy.transform.offset_x += dx;
        copy.transform.offset_y += dy;
        copy
    }

    pub fn add_polygon(&mut self, layer: impl Into<CompactString>, points: Vec<(i64, i64)>, net: Option<NetId>) {
        self.polygons.push(CellPolygon {
            layer: layer.into(),
            points,
            net,
        });
    }

    pub fn add_contact(
        &mut self,
        from: impl Into<CompactString>,
        to: impl Into<CompactString>,
        at: (i64, i64),
        diameter: i64,
        name: Option<CompactString>,
        net: Option<NetId>,
    ) {
        self.contacts.push(CellContact {
            name: name.unwrap_or_default(),
            from_layer: from.into(),
            to_layer: to.into(),
            at,
            diameter,
            net,
        });
    }

    pub fn add_port(
        &mut self,
        name: impl Into<CompactString>,
        at: (i64, i64),
        layer: impl Into<CompactString>,
        net: Option<NetId>,
    ) {
        self.ports.push(CellPort {
            name: name.into(),
            at,
            layer: layer.into(),
            net,
        });
    }

    pub fn add_device(
        &mut self,
        device_type: impl Into<CompactString>,
        instance_name: impl Into<CompactString>,
        terminals: Vec<(CompactString, CompactString)>,
        params: Vec<(CompactString, Value)>,
    ) {
        self.devices.push(CellDevice {
            device_type: device_type.into(),
            instance_name: instance_name.into(),
            terminals,
            params,
        });
    }

    pub fn place(&mut self, child: &CellLayout, at: (i64, i64)) {
        for poly in &child.polygons {
            let mut pts = Vec::with_capacity(poly.points.len());
            for pt in &poly.points {
                let (tx, ty) = child.transform.apply_point(*pt);
                pts.push((at.0 + tx, at.1 + ty));
            }
            self.polygons.push(CellPolygon {
                layer: poly.layer.clone(),
                points: pts,
                net: poly.net,
            });
        }
        for c in &child.contacts {
            let (tx, ty) = child.transform.apply_point(c.at);
            self.contacts.push(CellContact {
                name: c.name.clone(),
                from_layer: c.from_layer.clone(),
                to_layer: c.to_layer.clone(),
                at: (at.0 + tx, at.1 + ty),
                diameter: c.diameter,
                net: c.net,
            });
        }
        for port in &child.ports {
            let (tx, ty) = child.transform.apply_point(port.at);
            self.ports.push(CellPort {
                name: port.name.clone(),
                at: (at.0 + tx, at.1 + ty),
                layer: port.layer.clone(),
                net: port.net,
            });
        }
        for dev in &child.devices {
            self.devices.push(dev.clone());
        }
    }

    pub fn bounding_box(&self) -> (i64, i64, i64, i64) {
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for p in &self.polygons {
            for pt in &p.points {
                let (tx, ty) = self.transform.apply_point(*pt);
                min_x = min_x.min(tx);
                min_y = min_y.min(ty);
                max_x = max_x.max(tx);
                max_y = max_y.max(ty);
            }
        }
        for c in &self.contacts {
            let (tx, ty) = self.transform.apply_point(c.at);
            let r = c.diameter / 2;
            min_x = min_x.min(tx - r);
            min_y = min_y.min(ty - r);
            max_x = max_x.max(tx + r);
            max_y = max_y.max(ty + r);
        }
        for port in &self.ports {
            let (tx, ty) = self.transform.apply_point(port.at);
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
            max_x = max_x.max(tx);
            max_y = max_y.max(ty);
        }

        if min_x > max_x {
            (0, 0, 0, 0)
        } else {
            (min_x, min_y, max_x, max_y)
        }
    }
}

/// Placed cell instance in top-level space (World Coordinates)
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCellInstance {
    pub cell: CellLayout,
    pub placement_x: i64, // pm
    pub placement_y: i64, // pm
}

impl PlacedCellInstance {
    pub fn port(&self, port_name: &str) -> Option<Value> {
        self.cell.ports.iter().find(|p| p.name == port_name).map(|p| {
            let transformed_local = self.cell.transform.apply_point(p.at);
            let world_x = self.placement_x + transformed_local.0;
            let world_y = self.placement_y + transformed_local.1;
            Value::Point2D { x: world_x, y: world_y }
        })
    }

    pub fn bounding_box(&self) -> Value {
        let (lx, ly, hx, hy) = self.cell.bounding_box();
        Value::BoundingBox {
            min_x: self.placement_x + lx,
            min_y: self.placement_y + ly,
            max_x: self.placement_x + hx,
            max_y: self.placement_y + hy,
        }
    }
}

/// A physical measurement scaled to its dimension's canonical internal unit (7-Base SI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeasurementValue {
    /// 128-bit signed integer value scaled to internal base units (pm, nV, pA, uOhm, aF, pH, fs, etc.)
    pub raw: i128,
    pub dimension: SiDimension,
}

pub type PhysicalValue = MeasurementValue;

impl MeasurementValue {
    #[inline]
    pub const fn raw_value(&self) -> i128 {
        self.raw
    }

    pub const fn new(raw: i128, dimension: SiDimension) -> Self {
        Self { raw, dimension }
    }

    pub const fn length_pm(pm: i128) -> Self {
        Self::new(pm, SiDimension::LENGTH)
    }

    pub const fn voltage_nv(nv: i128) -> Self {
        Self::new(nv, SiDimension::VOLTAGE)
    }

    pub const fn current_pa(pa: i128) -> Self {
        Self::new(pa, SiDimension::CURRENT)
    }

    /// Convert from AST `Unit` and value using optional `UnitRegistry`.
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
                let nv = (base_v * 1_000_000_000.0).round() as i128;
                Some(Self::new(nv, SiDimension::VOLTAGE))
            }
            Unit::Current(c) => {
                let multiplier = match c {
                    hwc_parser::lexer::units::CurrentUnit::Amperes => 1.0,
                    hwc_parser::lexer::units::CurrentUnit::Milliamperes => 1e-3,
                    hwc_parser::lexer::units::CurrentUnit::Microamperes => 1e-6,
                };
                let base_a = val * multiplier;
                let pa = (base_a * 1_000_000_000_000.0).round() as i128;
                Some(Self::new(pa, SiDimension::CURRENT))
            }
            Unit::Temperature(_) => {
                let mk = (val * 1000.0).round() as i128;
                Some(Self::new(mk, SiDimension::TEMPERATURE))
            }
            Unit::Custom(s) => Self::from_unit_str(val, s, registry),
        }
    }

    /// Convert a numeric value and unit string using `UnitRegistry` or canonical SI fallback lookup.
    pub fn from_unit_str(val: f64, unit_str: &str, registry: Option<&UnitRegistry>) -> Option<Self> {
        // 1. If registry is provided, use it as the single source of truth
        if let Some(reg) = registry {
            if let Some(info) = reg.get(unit_str) {
                if let Some(dim) = info.si_dimension {
                    if let Some(multiplier) = info.multiplier {
                        let base_si = val * multiplier;
                        let raw = (base_si * Self::scale_for_dimension(dim)).round() as i128;
                        return Some(Self::new(raw, dim));
                    }
                }
            }
        }

        // 2. Canonical standard lookup fallback
        let reg = UnitRegistry::standard();
        if let Some(info) = reg.get(unit_str) {
            if let Some(dim) = info.si_dimension {
                if let Some(multiplier) = info.multiplier {
                    let base_si = val * multiplier;
                    let raw = (base_si * Self::scale_for_dimension(dim)).round() as i128;
                    return Some(Self::new(raw, dim));
                }
            }
        }

        None
    }

    pub fn scale_for_dimension(dim: SiDimension) -> f64 {
        if dim == SiDimension::LENGTH {
            1_000_000_000_000.0 // 1 m = 10^12 pm
        } else if dim == SiDimension::AREA {
            1_000_000_000_000_000_000_000_000.0 // 1 m^2 = 10^24 pm^2
        } else if dim == SiDimension::VOLUME {
            1_000_000_000_000_000_000_000_000_000_000_000_000.0 // 1 m^3 = 10^36 pm^3
        } else if dim == SiDimension::VOLTAGE {
            1_000_000_000.0 // 1 V = 10^9 nV
        } else if dim == SiDimension::CURRENT {
            1_000_000_000_000.0 // 1 A = 10^12 pA
        } else if dim == SiDimension::RESISTANCE || dim == SiDimension::SHEET_RES {
            1_000_000.0 // 1 Ohm = 10^6 uOhm
        } else if dim == SiDimension::CAPACITANCE || dim == SiDimension::CAPACITANCE_DENSITY {
            1_000_000_000_000_000_000.0 // 1 F = 10^18 aF
        } else if dim == SiDimension::INDUCTANCE {
            1_000_000_000_000.0 // 1 H = 10^12 pH
        } else if dim == SiDimension::TIME {
            1_000_000_000_000_000.0 // 1 s = 10^15 fs
        } else if dim == SiDimension::POWER {
            1_000_000_000_000.0 // 1 W = 10^12 pW
        } else if dim == SiDimension::TEMPERATURE {
            1_000.0 // 1 K = 10^3 mK
        } else if dim == SiDimension::ANGLE {
            1_000_000.0 // 1 deg = 10^6 udeg
        } else {
            1.0
        }
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
        let dim = self.dimension.mul(rhs.dimension);
        let raw = if (self.dimension == SiDimension::VOLTAGE && rhs.dimension == SiDimension::CURRENT)
            || (self.dimension == SiDimension::CURRENT && rhs.dimension == SiDimension::VOLTAGE)
        {
            // nV (10^-9) * pA (10^-12) = 10^-21 W. pW is 10^-12 W -> / 10^9
            (self.raw * rhs.raw) / 1_000_000_000
        } else if (self.dimension == SiDimension::CURRENT && rhs.dimension == SiDimension::RESISTANCE)
            || (self.dimension == SiDimension::RESISTANCE && rhs.dimension == SiDimension::CURRENT)
        {
            // pA (10^-12) * uOhm (10^-6) = 10^-18 V. nV is 10^-9 V -> / 10^9
            (self.raw * rhs.raw) / 1_000_000_000
        } else {
            self.raw * rhs.raw
        };
        Ok(Value::Measurement(Self { raw, dimension: dim }))
    }

    pub fn div_measurement(self, rhs: Self) -> Result<Value, EvalError> {
        if rhs.raw == 0 {
            return Err(EvalError::DivisionByZero);
        }
        if self.dimension == rhs.dimension {
            return Ok(Value::Float(self.raw as f64 / rhs.raw as f64));
        }
        let dim = self.dimension.div(rhs.dimension);
        let raw = if self.dimension == SiDimension::VOLTAGE && rhs.dimension == SiDimension::CURRENT {
            // nV (10^-9) / pA (10^-12) = 10^3 Ohm. uOhm is 10^-6 Ohm -> * 10^9
            (self.raw * 1_000_000_000) / rhs.raw
        } else if self.dimension == SiDimension::VOLTAGE && rhs.dimension == SiDimension::RESISTANCE {
            (self.raw * 1_000_000_000) / rhs.raw
        } else if self.dimension == SiDimension::POWER && rhs.dimension == SiDimension::CURRENT {
            (self.raw * 1_000_000_000) / rhs.raw
        } else if self.dimension == SiDimension::POWER && rhs.dimension == SiDimension::VOLTAGE {
            (self.raw * 1_000_000_000) / rhs.raw
        } else {
            self.raw / rhs.raw
        };
        Ok(Value::Measurement(Self { raw, dimension: dim }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeviceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

    // ── Pure Physical PCell & Placed Cell Handles (Pillar 1) ──
    CellLayout(Arc<CellLayout>),
    PlacedCell(Arc<PlacedCellInstance>),

    // ── Composites & References ──
    Array(Arc<Vec<Value>>),
    Tuple(Arc<Vec<Value>>),
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
                if m.dimension == SiDimension::LENGTH {
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
                } else if m.dimension == SiDimension::VOLTAGE {
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
                } else if m.dimension == SiDimension::CURRENT {
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
                } else if m.dimension == SiDimension::RESISTANCE || m.dimension == SiDimension::SHEET_RES {
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
                } else if m.dimension == SiDimension::CAPACITANCE || m.dimension == SiDimension::CAPACITANCE_DENSITY {
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
                } else if m.dimension == SiDimension::INDUCTANCE {
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
                } else if m.dimension == SiDimension::TIME {
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
                } else if m.dimension == SiDimension::FREQUENCY {
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
                } else if m.dimension == SiDimension::POWER {
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
                } else if m.dimension == SiDimension::ANGLE {
                    let udeg = m.raw;
                    write!(f, "{:.2}deg", udeg as f64 / 1e6)
                } else if m.dimension == SiDimension::TEMPERATURE {
                    let mk = m.raw;
                    write!(f, "{:.2}K", mk as f64 / 1e3)
                } else if m.dimension == SiDimension::AREA {
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
                } else if m.dimension == SiDimension::VOLUME {
                    let pm3 = m.raw;
                    write!(f, "{}pm^3", pm3)
                } else {
                    write!(f, "{:?}({})", m.dimension, m.raw)
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
            Value::CellLayout(cell) => {
                write!(f, "<CellLayout '{}' ({} polys, {} ports)>", cell.name, cell.polygons.len(), cell.ports.len())
            }
            Value::PlacedCell(inst) => {
                write!(f, "<PlacedCell '{}' at ({}, {})>", inst.cell.name, inst.placement_x, inst.placement_y)
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
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
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
