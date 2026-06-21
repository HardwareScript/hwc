//! Fabrication constraint loading from profiles.
//!
//! This module handles loading fabrication constraints from profile definitions
//! in the Symbol Table.

use super::symbol_table::{
    extract_clearance_constraints, extract_solder_mask_expansion, extract_stackup_constraints,
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

    // Extract solder mask expansion (v0.1.7)
    let solder_mask_expansion_nm = extract_solder_mask_expansion(profile_def, symbol_table)?;

    Ok(FabricationConstraints {
        min_trace_width_nm,
        min_trace_spacing_nm,
        min_via_diameter_nm,
        default_via_diameter_nm,
        min_annular_ring_nm,
        min_spacing_nm,
        high_voltage_clearance_nm: clearance.map(|(hv, _)| hv),
        safety_factor: clearance.map(|(_, sf)| sf).unwrap_or(2.0),
        stackup,
        solder_mask_expansion_nm,
        technology: profile_def.technology.clone(),
    })
}
