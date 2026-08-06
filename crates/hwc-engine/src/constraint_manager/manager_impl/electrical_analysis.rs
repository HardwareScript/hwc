//! Electrical analysis for nets.
//!
//! This module provides functions for analyzing the electrical properties of nets,
//! including voltage and current determination based on net declarations.
//!
//! **No hardcoded heuristics.** All electrical properties MUST come from
//! explicit `nets:` declarations in the space definition. The compiler does
//! NOT guess voltages or currents from net name patterns.

use crate::netlist::{NetData, NetlistArena};

/// Analyze electrical properties of a net.
///
/// Determines voltage from the net's declaration in the space definition's
/// `nets:` block. Returns current as Option — None means no current declared.
///
/// # Arguments
/// * `net` - Net data from the netlist
/// * `_netlist` - Full netlist arena for component lookup
/// * `net_declaration` - Optional declaration from the space definition's `nets:` block
/// * `unit_registry` - Registry for unit conversion (voltage/current dimensions)
///
/// # Returns
/// Tuple of (voltage_mv, Option<current_ma>), or error if no declaration is provided
pub fn analyze_net_electrical(
    net: &NetData,
    _netlist: &NetlistArena,
    net_declaration: Option<&hwc_parser::NetDeclaration>,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<(i64, Option<i64>), String> {
    let decl = net_declaration.ok_or_else(|| {
        format!(
            "Net '{}' has no electrical specification. Add a 'nets:' declaration with classification and potential.",
            net.name
        )
    })?;

    let voltage_mv = decl
        .potential
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Net '{}' has a declaration but no potential specified. Add 'potential: <voltage>' to the nets: entry.",
                net.name
            )
        })?
        .to_millivolts(unit_registry)
        .map_err(|e| format!("Failed to convert voltage for net '{}': {}", net.name, e))?;

    // v0.1.8: Use declared current from NetDeclaration.
    // If not declared, return None (caller will handle defaults/errors).
    let current_ma = decl
        .current
        .as_ref()
        .and_then(|c| c.to_milliamperes(unit_registry).ok())
        .map(|ma| ma as i64);

    Ok((voltage_mv, current_ma))
}
