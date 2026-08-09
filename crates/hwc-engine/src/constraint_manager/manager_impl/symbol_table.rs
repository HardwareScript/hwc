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

/// Extract trace constraints from profile definition.
///
/// Converts trace constraints (min_width, min_spacing) from profile
/// to nanometers for use in routing.
///
/// # Arguments
/// * `profile` - Profile definition from Symbol Table
/// * `symbol_table` - Symbol table for unit conversion
///
/// # Returns
/// Tuple of (min_width_nm, min_spacing_nm), or error if constraints missing
pub fn extract_trace_constraints<S: SymbolTableTrait>(
    profile: &ProfileDefinition,
    symbol_table: &S,
) -> Result<(i64, i64), String> {
    let trace = profile
        .trace
        .as_ref()
        .ok_or_else(|| format!("Profile '{}': missing trace constraints", profile.name))?;

    let min_width_nm = symbol_table.measurement_to_nm(&trace.min_width)?;
    let min_spacing_nm = symbol_table.measurement_to_nm(&trace.min_spacing)?;

    Ok((min_width_nm, min_spacing_nm))
}

/// Extract via constraints from profile definition.
///
/// Converts via constraints (min_diameter, min_annular_ring, min_spacing) from profile
/// to nanometers for use in routing.
///
/// # Arguments
/// * `profile` - Profile definition from Symbol Table
/// * `symbol_table` - Symbol table for unit conversion
///
/// # Returns
/// Tuple of (min_diameter_nm, default_diameter_nm, min_annular_ring_nm, min_spacing_nm), or error if constraints missing
pub fn extract_via_constraints<S: SymbolTableTrait>(
    profile: &ProfileDefinition,
    symbol_table: &S,
) -> Result<(i64, i64, i64, i64), String> {
    let via = profile
        .via
        .as_ref()
        .ok_or_else(|| format!("Profile '{}': missing via constraints", profile.name))?;

    let min_diameter_nm = symbol_table.measurement_to_nm(&via.min_diameter)?;
    let default_diameter_nm = if let Some(default) = &via.default_diameter {
        symbol_table.measurement_to_nm(default)?
    } else {
        min_diameter_nm
    };
    let min_annular_ring_nm = symbol_table.measurement_to_nm(&via.min_annular_ring)?;

    // v0.1.7: Extract drill-to-drill spacing
    let min_spacing_nm = if let Some(spacing) = &via.min_spacing {
        symbol_table.measurement_to_nm(spacing)?
    } else {
        return Err(format!(
            "Profile '{}': via.min_spacing must be explicitly declared. No implicit defaults permitted.",
            profile.name
        ));
    };

    Ok((
        min_diameter_nm,
        default_diameter_nm,
        min_annular_ring_nm,
        min_spacing_nm,
    ))
}

/// Extract clearance constraints from profile definition.
///
/// Extracts high voltage clearance and safety factor from profile.
///
/// # Arguments
/// * `profile` - Profile definition from Symbol Table
/// * `symbol_table` - Symbol table for unit conversion
///
/// # Returns
/// Tuple of (high_voltage_clearance_nm, safety_factor), or None if not specified
pub fn extract_clearance_constraints<S: SymbolTableTrait>(
    profile: &ProfileDefinition,
    symbol_table: &S,
) -> Result<Option<(i64, i64, i64, f64)>, String> {
    if let Some(clearance) = &profile.clearance {
        let high_voltage_nm = clearance
            .high_voltage
            .as_ref()
            .map(|m| symbol_table.measurement_to_nm(m))
            .transpose()?
            .unwrap_or(0);

        let safety_factor = clearance.safety_factor.unwrap_or(2.0);

        // v0.1.8: Low/medium voltage clearance derived from trace.min_spacing.
        // The profile's trace.min_spacing is the standard net-to-net spacing,
        // appropriate for low/medium voltage nets.
        let default_clearance = profile
            .trace
            .as_ref()
            .map(|t| symbol_table.measurement_to_nm(&t.min_spacing))
            .transpose()?
            .unwrap_or(0);

        Ok(Some((
            default_clearance,
            default_clearance,
            high_voltage_nm,
            safety_factor,
        )))
    } else {
        Ok(None)
    }
}

/// Extract stackup constraints from profile definition.
///
/// **v0.1.7 Breaking Change**: The old impedance-only `StackupConstraints`
/// has been removed in favor of the physical `LayerStackup`.
///
/// This function currently returns `None`. Proper derivation of dielectric
/// height, permittivity, etc. from the new physical stackup (plus Material
/// Database) will be implemented as part of the `StackupManager` in Phase 3.
///
/// The physical `LayerStackup` is now the single source of truth.
pub fn extract_stackup_constraints<S: SymbolTableTrait>(
    _profile: &ProfileDefinition,
    _symbol_table: &S,
) -> Result<Option<crate::constraint_manager::types::StackupInfo>, String> {
    // Old impedance stackup fields (dielectric_height, etc.) no longer exist.
    // See Phase 2 "Rip Off the Band-Aid" decision in the roadmap.
    Ok(None)
}
