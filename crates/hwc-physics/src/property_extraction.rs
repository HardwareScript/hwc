//! Property extraction helpers for material definitions.
//!
//! This module provides utilities to extract physical properties from MaterialDecl
//! AST nodes with proper unit handling and error reporting.

use compact_str::CompactString;
use hwc_parser::{Expression, MaterialDecl, Unit};
use thiserror::Error;

/// Errors that can occur during property extraction
#[derive(Error, Debug)]
pub enum PropertyError {
    #[error("Missing required property '{property}' in material '{material}'")]
    MissingProperty {
        material: CompactString,
        property: String,
    },

    #[error("Invalid unit for property '{property}' in material '{material}': expected {expected}, got {actual}")]
    InvalidUnit {
        material: CompactString,
        property: CompactString,
        expected: CompactString,
        actual: CompactString,
    },

    #[error(
        "Invalid property type for '{property}' in material '{material}': expected measurement"
    )]
    InvalidPropertyType {
        material: CompactString,
        property: String,
    },
}

/// Extract resistivity from material definition (in Ω·m)
///
/// Accepts:
/// - Custom("Ω·m") or Custom("Ohm·m")
/// - Custom("Ω/m") (legacy, will convert)
///
/// Returns resistivity in ohm-meters (Ω·m)
pub fn extract_resistivity(material: &MaterialDecl) -> Result<f64, PropertyError> {
    let expr = find_property(&material.properties, "resistivity").ok_or_else(|| {
        PropertyError::MissingProperty {
            material: material.name.name.clone(),
            property: "resistivity".into(),
        }
    })?;

    match expr {
        Expression::Measurement { value, unit, .. } => {
            match unit {
                Unit::Custom(unit_str) if unit_str == "Ω·m" || unit_str == "Ohm·m" || unit_str == "Ω/m" => Ok(*value),
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "resistivity".into(),
                    expected: "Ω·m".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "resistivity".into(),
                    expected: "Ω·m".into(),
                    actual: format!("{:?}", unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.name.clone(),
            property: "resistivity".into(),
        }),
    }
}

/// Extract thermal conductivity from material definition (in W/mK)
///
/// Accepts:
/// - Custom("W/mK") or Custom("W/m·K")
///
/// Returns thermal conductivity in watts per meter-kelvin (W/mK)
pub fn extract_thermal_conductivity(material: &MaterialDecl) -> Result<f64, PropertyError> {
    let expr =
        find_property(&material.properties, "thermal_conductivity").ok_or_else(|| {
            PropertyError::MissingProperty {
                material: material.name.name.clone(),
                property: "thermal_conductivity".into(),
            }
        })?;

    match expr {
        Expression::Measurement { value, unit, .. } => {
            match unit {
                Unit::Custom(unit_str) if unit_str == "W/mK" || unit_str == "W/m·K" => Ok(*value),
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "thermal_conductivity".into(),
                    expected: "W/mK".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "thermal_conductivity".into(),
                    expected: "W/mK".into(),
                    actual: format!("{:?}", unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.name.clone(),
            property: "thermal_conductivity".into(),
        }),
    }
}

/// Extract relative permittivity from material definition (dimensionless)
///
/// Accepts:
/// - Number (dimensionless)
/// - Measurement with Custom("") (dimensionless)
///
/// Returns relative permittivity (dimensionless)
pub fn extract_relative_permittivity(material: &MaterialDecl) -> Result<f64, PropertyError> {
    let expr =
        find_property(&material.properties, "relative_permittivity").ok_or_else(|| {
            PropertyError::MissingProperty {
                material: material.name.name.clone(),
                property: "relative_permittivity".into(),
            }
        })?;

    match expr {
        Expression::Literal { value, .. } => Ok(*value as f64),
        Expression::FloatLiteral { value, .. } => Ok(*value),
        Expression::Measurement { value, .. } => Ok(*value),
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.name.clone(),
            property: "relative_permittivity".into(),
        }),
    }
}

/// Extract dielectric strength from material definition (in kV/mm)
///
/// Accepts:
/// - Custom("kV/mm")
///
/// Returns dielectric strength in kilovolts per millimeter (kV/mm)
pub fn extract_dielectric_strength(material: &MaterialDecl) -> Result<f64, PropertyError> {
    let expr = find_property(&material.properties, "dielectric_strength").ok_or_else(|| {
        PropertyError::MissingProperty {
            material: material.name.name.clone(),
            property: "dielectric_strength".into(),
        }
    })?;

    match expr {
        Expression::Measurement { value, unit, .. } => {
            match unit {
                Unit::Custom(unit_str) if unit_str == "kV/mm" => Ok(*value),
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "dielectric_strength".into(),
                    expected: "kV/mm".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.name.clone(),
                    property: "dielectric_strength".into(),
                    expected: "kV/mm".into(),
                    actual: format!("{:?}", unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.name.clone(),
            property: "dielectric_strength".into(),
        }),
    }
}

/// Helper function to find a property expression by key
fn find_property<'a>(properties: &'a [(CompactString, Expression)], key: &str) -> Option<&'a Expression> {
    properties.iter().find(|(k, _)| k == key).map(|(_, expr)| expr)
}
