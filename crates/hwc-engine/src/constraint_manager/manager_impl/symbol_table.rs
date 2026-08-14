//! Symbol Table integration and material/profile property extraction.
//!
//! This module provides the trait for Symbol Table access and helper functions
//! for extracting material and profile properties needed for constraint generation.

use hwc_parser::{MaterialDefinition, ProfileDefinition, PropertyValue, Unit};

// ============================================================================
// Symbol Table Trait
// ============================================================================

/// Trait for Symbol Table access (dependency inversion).
///
/// This trait allows the constraint manager to access material and profile definitions
/// without depending directly on hwc-compiler (which would create a circular dependency).
pub trait SymbolTableTrait {
    /// Get a material definition by name
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String>;

    /// Get a profile definition by name
    fn get_profile(&self, name: &str) -> Result<&ProfileDefinition, String>;

    /// **CANONICAL UNIT CONVERSION METHOD**
    ///
    /// Convert a measurement to nanometers. This is the SINGLE SOURCE OF TRUTH
    /// for all unit conversions. Every part of the compiler/engine that needs to
    /// convert measurements MUST use this method.
    ///
    /// # Returns
    /// Value in nanometers, or error if the unit cannot be resolved or is not a length unit
    fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String>;
}

// ============================================================================
// Material Property Extraction
// ============================================================================

/// Extract dielectric strength from material definition.
///
/// Searches the material's properties for "dielectric_strength" and converts
/// the value to kV/mm for use in clearance calculations.
///
/// # Arguments
/// * `material` - Material definition from Symbol Table
///
/// # Returns
/// Dielectric strength in kV/mm, or error if property is missing or invalid
///
/// # Errors
/// Returns error if:
/// - Material has no "dielectric_strength" property
/// - Property value is not a measurement
/// - Property unit cannot be converted to kV/mm
pub fn extract_dielectric_strength(material: &MaterialDefinition) -> Result<f64, String> {
    // Search for dielectric_strength property
    for prop in &material.properties {
        if prop.key == "dielectric_strength" {
            match &prop.value {
                PropertyValue::Measurement(measurement) => {
                    // Convert measurement to kV/mm based on unit
                    let value_kv_mm = match &measurement.unit {
                        // Already in kV/mm - perfect!
                        Unit::Custom(unit_str) if unit_str == "kV/mm" => measurement.value,

                        // V/mm → kV/mm (divide by 1000)
                        Unit::Custom(unit_str) if unit_str == "V/mm" => measurement.value / 1000.0,

                        // MV/mm → kV/mm (multiply by 1000)
                        Unit::Custom(unit_str) if unit_str == "MV/mm" => measurement.value * 1000.0,

                        // Unknown unit - return error with helpful message
                        _ => {
                            return Err(format!(
                                "Material '{}': dielectric_strength has unsupported unit '{:?}'. Expected kV/mm, V/mm, or MV/mm",
                                material.name,
                                measurement.unit
                            ));
                        }
                    };

                    return Ok(value_kv_mm);
                }
                _ => {
                    return Err(format!(
                        "Material '{}': dielectric_strength must be a measurement (e.g., 20kV/mm), found {:?}",
                        material.name,
                        prop.value
                    ));
                }
            }
        }
    }

    // Property not found
    Err(format!(
        "Material '{}': missing required property 'dielectric_strength'",
        material.name
    ))
}

// ============================================================================
// Unit Conversion
// ============================================================================

// ============================================================================
// Profile Constraint Extraction
// ============================================================================

// NOTE: Profile constraint extraction (`extract_trace_constraints`,
// `extract_via_constraints`, `extract_clearance_constraints`,
// `extract_stackup_constraints`) was removed. Constraint generation now reads
// profile fields directly via the `SymbolTableTrait` without these helpers.
