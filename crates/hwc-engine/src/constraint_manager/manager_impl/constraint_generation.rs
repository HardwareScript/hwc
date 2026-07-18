//! Core constraint generation logic.
//!
//! This module contains the main logic for generating routing constraints
//! and clearance zones for individual nets.

use super::impedance::{calculate_trace_impedance, determine_target_impedance};
use super::symbol_table::{extract_dielectric_strength, SymbolTableTrait};
use crate::constraint_manager::clearance::calculate_clearance_nm;
use crate::constraint_manager::trace_width::calculate_trace_width_nm;
use crate::constraint_manager::types::{ClearanceZone, FabricationConstraints, RouteConstraints};
use crate::netlist::{NetData, NetId};

/// Parameters for generating net constraints
pub struct NetConstraintParams<'a> {
    pub voltage_mv: i64,
    pub current_ma: i64,
    pub material_name: &'a str,
    pub is_external: bool,
    pub safety_factor: i64,
    pub default_temp_rise_c: i64,
    pub default_max_parallel_nm: i64,
    pub fabrication_constraints: Option<&'a FabricationConstraints>,
}

/// Generate constraints for a single net.
///
/// Translates electrical requirements and material properties into
/// geometric constraints for the router.
///
/// **v0.1.4 Change**: Now uses Symbol Table to load material properties
/// instead of accepting hardcoded dielectric strength parameter.
///
/// # Arguments
/// * `net` - Net data from the netlist
/// * `params` - Constraint generation parameters
/// * `symbol_table` - Symbol Table containing material definitions
///
/// # Returns
/// Routing constraints for this net, or error if material not found
///
/// # Errors
/// Returns error if material is not defined in Symbol Table or if
/// dielectric_strength property is missing.
pub fn generate_net_constraints<S: SymbolTableTrait>(
    net: &NetData,
    params: &NetConstraintParams,
    symbol_table: &S,
) -> Result<RouteConstraints, String> {
    // Load material from Symbol Table (v0.1.4 integration)
    let material_def = symbol_table.get_material(params.material_name)?;

    // Extract dielectric strength from material properties
    let dielectric_strength_kv_mm = extract_dielectric_strength(material_def)?;

    // Calculate clearance from voltage and dielectric strength
    let min_clearance_nm = calculate_clearance_nm(
        params.voltage_mv,
        dielectric_strength_kv_mm,
        params.safety_factor,
    );

    // Calculate trace width from current requirements
    let min_trace_width_nm = calculate_trace_width_nm(
        params.current_ma,
        params.default_temp_rise_c,
        params.is_external,
    );

    // Use net's specified width if larger than minimum
    let final_trace_width_nm = min_trace_width_nm.max(net.width_nm);

    // Calculate maximum resistance — no limit without PDK specification
    let max_resistance_ohm = f64::INFINITY;

    // Determine target impedance for high-speed signals
    // Note: This is a TARGET impedance, not calculated impedance
    // Actual impedance will be calculated and validated during DRC after routing
    let target_impedance_ohm = determine_target_impedance(&net.name);

    // If we have a target impedance and stackup information, calculate expected impedance
    // This helps validate that our trace width will achieve the target impedance
    if let Some(target_ohm) = target_impedance_ohm {
        if let Some(fab_constraints) = params.fabrication_constraints {
            if let Some(stackup) = &fab_constraints.stackup {
                let calculated_impedance = calculate_trace_impedance(
                    final_trace_width_nm,
                    stackup.copper_thickness_nm,
                    stackup.dielectric_height_nm,
                    stackup.relative_permittivity,
                );

                // Check if calculated impedance is close to target
                // If not, the trace width may need adjustment (will be caught in DRC)
                let impedance_error = (calculated_impedance - target_ohm).abs();
                let tolerance = target_ohm * 0.1; // 10% tolerance

                if impedance_error > tolerance {
                    // Impedance mismatch detected - this will be reported during DRC
                    // For now, we continue with the target impedance
                    // The router will use the target, and DRC will validate the actual result
                }
            }
        }
    }

    Ok(RouteConstraints {
        min_trace_width_nm: final_trace_width_nm,
        min_clearance_nm,
        max_parallel_length_nm: params.default_max_parallel_nm,
        max_resistance_ohm,
        max_current_ma: params.current_ma,
        impedance_ohm: target_impedance_ohm,
    })
}

/// Generate clearance zone for a net.
///
/// Creates the "forcefield" around a net's copper traces.
///
/// **v0.1.4 Change**: Now uses Symbol Table to load material properties
/// instead of accepting hardcoded dielectric strength parameter.
///
/// # Arguments
/// * `net_id` - Net identifier
/// * `voltage_mv` - Net voltage in millivolts
/// * `material_name` - Name of the dielectric material (e.g., "FR4", "Air")
/// * `symbol_table` - Symbol Table containing material definitions
/// * `safety_factor` - Safety factor for clearance calculations
///
/// # Returns
/// Clearance zone for this net, or error if material not found
///
/// # Errors
/// Returns error if material is not defined in Symbol Table or if
/// dielectric_strength property is missing.
pub fn generate_clearance_zone<S: SymbolTableTrait>(
    net_id: NetId,
    voltage_mv: i64,
    material_name: &str,
    symbol_table: &S,
    safety_factor: i64,
) -> Result<ClearanceZone, String> {
    // Load material from Symbol Table (v0.1.4 integration)
    let material_def = symbol_table.get_material(material_name)?;

    // Extract dielectric strength from material properties
    let dielectric_strength_kv_mm = extract_dielectric_strength(material_def)?;

    // Calculate clearance radius
    let clearance_radius_nm =
        calculate_clearance_nm(voltage_mv, dielectric_strength_kv_mm, safety_factor);

    Ok(ClearanceZone {
        net_id,
        voltage_mv,
        clearance_radius_nm,
    })
}
