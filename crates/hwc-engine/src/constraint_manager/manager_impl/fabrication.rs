//! Fabrication constraint loading from profiles.
//!
//! This module handles loading fabrication constraints from profile definitions
//! in the Symbol Table.

use super::symbol_table::{
    extract_clearance_constraints, extract_stackup_constraints,
    extract_trace_constraints, extract_via_constraints, SymbolTableTrait,
};
use crate::constraint_manager::types::FabricationConstraints;

/// Load fabrication constraints from profile definition.
///
/// Extracts trace widths, via sizes, and clearances from a profile
/// definition in the Symbol Table.
///
/// **v0.1.4 Change**: Loads from Symbol Table instead of YAML files.
///
/// # Arguments
/// * `profile_name` - Name of the profile (e.g., "PCB_Standard")
/// * `symbol_table` - Symbol Table containing profile definitions
///
/// # Returns
/// Fabrication constraints extracted from profile, or error if profile not found
///
/// # Errors
/// Returns error if:
/// - Profile is not defined in Symbol Table
/// - Profile is missing required constraints (trace, via)
/// - Measurement units cannot be converted to nanometers
///
/// # Example
/// Loads fabrication constraints from a profile definition in the symbol table.
pub fn load_fabrication_constraints<S: SymbolTableTrait>(
    profile_name: &str,
    symbol_table: &S,
) -> Result<FabricationConstraints, String> {
    // Load profile from Symbol Table (v0.1.4 integration)
    let profile_def = symbol_table.get_profile(profile_name)?;

    // Extract trace constraints (required)
    let (min_trace_width_nm, min_trace_spacing_nm) =
        extract_trace_constraints(profile_def, symbol_table)?;

    // Extract via constraints (required)
    let (min_via_diameter_nm, default_via_diameter_nm, min_annular_ring_nm, min_spacing_nm) =
        extract_via_constraints(profile_def, symbol_table)?;

    // Extract clearance constraints (optional)
    let clearance = extract_clearance_constraints(profile_def, symbol_table)?;

    // Extract stackup constraints (optional)
    let stackup = extract_stackup_constraints(profile_def, symbol_table)?;

    let (low_voltage_clearance_nm, medium_voltage_clearance_nm, high_voltage_clearance_nm, safety_factor) =
        clearance.ok_or_else(|| format!(
            "Profile '{}': missing clearance constraints. All clearance values must be explicitly declared in your PDK profile.",
            profile_name
        ))?;

    Ok(FabricationConstraints {
        min_trace_width_nm,
        min_trace_spacing_nm,
        min_via_diameter_nm,
        default_via_diameter_nm,
        min_annular_ring_nm,
        min_spacing_nm,
        low_voltage_clearance_nm,
        medium_voltage_clearance_nm,
        high_voltage_clearance_nm,
        safety_factor,
        stackup,
        technology: profile_def.technology.ok_or_else(|| {
            format!(
                "Profile '{}': missing REQUIRED 'technology' field. Must be explicitly declared as either 'PCB' or 'ASIC'.\n\
                 This field is mandatory because it determines via geometry, clearance rules, and manufacturing constraints.",
                profile_name
            )
        })?,
    })
}
