use hwc_parser::ast::Unit;
use hwc_parser::lexer::units::{CurrentUnit, VoltageUnit};
use hwc_types::{SiDimension, UnitRegistry};
use super::super::context::EvalError;
use super::Value;

/// A physical measurement scaled to its dimension's canonical internal unit (7-Base SI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeasurementValue {
    /// 128-bit signed integer value scaled to internal base units (pm, nV, pA, uOhm, aF, pH, fs, etc.)
    pub raw: i128,
    pub dimension: SiDimension,
}

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
                    VoltageUnit::Volts => 1.0,
                    VoltageUnit::Millivolts => 1e-3,
                    VoltageUnit::Kilovolts => 1e3,
                };
                let base_v = val * multiplier;
                let nv = (base_v * 1_000_000_000.0).round() as i128;
                Some(Self::new(nv, SiDimension::VOLTAGE))
            }
            Unit::Current(c) => {
                let multiplier = match c {
                    CurrentUnit::Amperes => 1.0,
                    CurrentUnit::Milliamperes => 1e-3,
                    CurrentUnit::Microamperes => 1e-6,
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
