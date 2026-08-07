//! Evaluated values and the evaluation context.

use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::ast::Unit;

/// Result of evaluating an expression
///
/// ## Type System Invariant: Preserve Unit Information Throughout Compilation
///
/// The compiler maintains strict dimensional correctness by storing typed values:
/// - **`Value::Number`**: Dimensionless integers (loop indices, multipliers, counts)
/// - **`Value::Float`**: Dimensionless floating-point numbers (ratios, scaling factors)
/// - **`Value::Measurement`**: Physical quantities with explicit units (50µm, 200nm, 1mm)
/// - **`Value::Percentage`**: Relative positioning values (50%, 25%)
///
/// PDK constants like `pdk.edge_clearance` are stored as `Value::Measurement` with
/// their original units preserved. This ensures expressions like `pdk.edge_clearance + 200µm`
/// are evaluated with full dimensional analysis, preventing mathematically invalid operations
/// like adding bare scalars to physical distances.
///
/// Final conversion to absolute nanometer coordinates happens in `conversions.rs` via
/// `to_nanometers()`, maintaining a clean separation between the parser (AST/evaluation)
/// and the physical engine (coordinate resolution).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Integer value (grid index or dimensionless number)
    Number(i64),
    /// Float value (multiplier or ratio) (v0.1.7)
    Float(f64),
    /// Physical measurement with unit
    Measurement { value: f64, unit: Unit },
    /// Percentage value (for relative positioning)
    Percentage(f64),
}

/// Scale a measurement to nanometers, or report an unsupported unit.
fn measurement_to_nanometers(value: f64, unit: &Unit) -> Result<i64, String> {
    match unit {
        Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
        Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
        Unit::Micrometer => Ok((value * 1_000.0) as i64),
        Unit::Nanometer => Ok(value as i64),
        Unit::Picometer => Ok((value * 0.001) as i64),
        _ => Err(format!("Cannot convert {:?} to nanometers", unit)),
    }
}

/// Scale a measurement to picometers, or report an unsupported unit.
fn measurement_to_picometers(value: f64, unit: &Unit) -> Result<i64, String> {
    match unit {
        Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
        Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
        Unit::Micrometer => Ok((value * 1_000.0) as i64),
        Unit::Nanometer => Ok((value * 1_000.0) as i64),
        Unit::Picometer => Ok(value as i64),
        _ => Err(format!("Cannot convert {:?} to picometers", unit)),
    }
}

impl Value {
    /// Convert to float, supporting both Number and Float variants
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n as f64),
            Value::Float(f) => Ok(*f),
            Value::Measurement { .. } => Err("Expected number but got measurement".into()),
            Value::Percentage(_) => Err("Expected number but got percentage".into()),
        }
    }

    /// Convert to integer, failing if this is a measurement or percentage
    pub fn as_integer(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { .. } => Err("Expected integer but got measurement".into()),
            Value::Percentage(_) => Err("Expected integer but got percentage".into()),
        }
    }

    /// Convert to nanometers
    /// For percentages, requires the reference dimension
    pub fn to_nanometers(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n), // Already a number, assume it's in nm if used as distance
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => measurement_to_nanometers(*value, unit),
            Value::Percentage(_) => {
                Err("Cannot convert percentage to nanometers without reference dimension".into())
            }
        }
    }

    /// Convert to nanometers with a reference dimension (for percentages)
    pub fn to_nanometers_with_ref(&self, reference_nm: i64) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => measurement_to_nanometers(*value, unit),
            Value::Percentage(pct) => {
                // Convert percentage to nanometers: 50% of 100mm = 50mm
                Ok(((pct / 100.0) * reference_nm as f64) as i64)
            }
        }
    }

    /// Convert to picometers (i64) — the engine's internal coordinate representation.
    /// Maximum addressable range: +/-9,220 km.
    pub fn to_picometers(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n), // Already a number, assume pm if used as distance
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => measurement_to_picometers(*value, unit),
            Value::Percentage(_) => {
                Err("Cannot convert percentage to picometers without reference dimension".into())
            }
        }
    }

    /// Convert to picometers with a reference dimension (for percentages)
    pub fn to_picometers_with_ref(&self, reference_pm: i64) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => measurement_to_picometers(*value, unit),
            Value::Percentage(pct) => Ok(((pct / 100.0) * reference_pm as f64) as i64),
        }
    }

    /// Check if this is a measurement
    pub fn is_measurement(&self) -> bool {
        matches!(self, Value::Measurement { .. })
    }

    /// Check if this is a percentage
    pub fn is_percentage(&self) -> bool {
        matches!(self, Value::Percentage(_))
    }

    /// Check if this is a measurement or percentage (valid for X/Y coordinates)
    pub fn is_physical_or_relative(&self) -> bool {
        matches!(self, Value::Measurement { .. } | Value::Percentage(_))
    }
}

/// Context for evaluating expressions with strongly-typed variable bindings
///
/// ## Architectural Principle: Preserve Unit Information Throughout Compilation
///
/// This context stores `Value` enums (not bare `i64`) to maintain dimensional correctness:
/// - **Value::Number**: Dimensionless scalars (loop counters, array indices, multipliers)
/// - **Value::Measurement**: Physical quantities with units (50µm, 200nm, pdk.edge_clearance)
/// - **Value::Percentage**: Relative positioning (50%, 25%)
///
/// This ensures the type system prevents mathematically invalid operations like
/// adding a bare scalar to a physical distance, and keeps unit metadata intact
/// throughout the entire compilation pipeline until final coordinate resolution.
pub type EvaluationContext = FxHashMap<CompactString, Value>;
