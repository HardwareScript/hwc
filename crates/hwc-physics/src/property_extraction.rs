//! Property extraction helpers for material definitions.
//!
//! This module provides utilities to extract physical properties from MaterialDefinition
//! AST nodes with proper unit handling and error reporting.

use compact_str::CompactString;
use hwc_parser::{MaterialDefinition, Property, PropertyValue, Unit};
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
pub fn extract_resistivity(material: &MaterialDefinition) -> Result<f64, PropertyError> {
    let property = find_property(&material.properties, "resistivity").ok_or_else(|| {
        PropertyError::MissingProperty {
            material: material.name.to_string().into(),
            property: "resistivity".into(),
        }
    })?;

    match &property.value {
        PropertyValue::Measurement(measurement) => {
            let value = measurement.value;
            match &measurement.unit {
                Unit::Custom(unit_str) if unit_str == "Ω·m" || unit_str == "Ohm·m" => Ok(value),
                Unit::Custom(unit_str) if unit_str == "Ω/m" => {
                    // Legacy notation, treat as Ω·m
                    Ok(value)
                }
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "resistivity".into(),
                    expected: "Ω·m".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "resistivity".into(),
                    expected: "Ω·m".into(),
                    actual: format!("{:?}", measurement.unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.to_string().into(),
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
pub fn extract_thermal_conductivity(material: &MaterialDefinition) -> Result<f64, PropertyError> {
    let property =
        find_property(&material.properties, "thermal_conductivity").ok_or_else(|| {
            PropertyError::MissingProperty {
                material: material.name.to_string().into(),
                property: "thermal_conductivity".into(),
            }
        })?;

    match &property.value {
        PropertyValue::Measurement(measurement) => {
            let value = measurement.value;
            match &measurement.unit {
                Unit::Custom(unit_str) if unit_str == "W/mK" || unit_str == "W/m·K" => Ok(value),
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "thermal_conductivity".into(),
                    expected: "W/mK".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "thermal_conductivity".into(),
                    expected: "W/mK".into(),
                    actual: format!("{:?}", measurement.unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.to_string().into(),
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
pub fn extract_relative_permittivity(material: &MaterialDefinition) -> Result<f64, PropertyError> {
    let property =
        find_property(&material.properties, "relative_permittivity").ok_or_else(|| {
            PropertyError::MissingProperty {
                material: material.name.to_string().into(),
                property: "relative_permittivity".into(),
            }
        })?;

    match &property.value {
        PropertyValue::Number(value) => Ok(*value),
        PropertyValue::Measurement(measurement) => {
            // Accept dimensionless measurements
            Ok(measurement.value)
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.to_string().into(),
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
pub fn extract_dielectric_strength(material: &MaterialDefinition) -> Result<f64, PropertyError> {
    let property = find_property(&material.properties, "dielectric_strength").ok_or_else(|| {
        PropertyError::MissingProperty {
            material: material.name.to_string().into(),
            property: "dielectric_strength".into(),
        }
    })?;

    match &property.value {
        PropertyValue::Measurement(measurement) => {
            let value = measurement.value;
            match &measurement.unit {
                Unit::Custom(unit_str) if unit_str == "kV/mm" => Ok(value),
                Unit::Custom(unit_str) => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "dielectric_strength".into(),
                    expected: "kV/mm".into(),
                    actual: unit_str.clone().into(),
                }),
                _ => Err(PropertyError::InvalidUnit {
                    material: material.name.to_string().into(),
                    property: "dielectric_strength".into(),
                    expected: "kV/mm".into(),
                    actual: format!("{:?}", measurement.unit).into(),
                }),
            }
        }
        _ => Err(PropertyError::InvalidPropertyType {
            material: material.name.to_string().into(),
            property: "dielectric_strength".into(),
        }),
    }
}

/// Helper function to find a property by key
fn find_property<'a>(properties: &'a [Property], key: &str) -> Option<&'a Property> {
    properties.iter().find(|p| p.key == key)
}
