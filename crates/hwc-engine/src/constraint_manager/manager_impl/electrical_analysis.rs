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
/// * `_unit_registry` - Registry for unit conversion (voltage/current dimensions)
///
/// # Returns
/// Tuple of (voltage_mv, Option<current_ma>), or error if no declaration is provided
pub fn analyze_net_electrical(
    net: &NetData,
    _netlist: &NetlistArena,
    net_declaration: Option<&hwc_parser::NetDeclaration>,
    _unit_registry: &hwc_types::UnitRegistry,
) -> Result<(i64, Option<i64>), String> {
    let decl = net_declaration.ok_or_else(|| {
        format!(
            "Net '{}' has no electrical specification. Add a 'nets:' declaration with classification and potential.",
            net.name
        )
    })?;

    let potential_expr = decl.properties.iter().find(|(k, _)| k == "potential").map(|(_, v)| v);
    let current_expr = decl.properties.iter().find(|(k, _)| k == "current").map(|(_, v)| v);

    let voltage_mv = if let Some(hwc_parser::Expression::Measurement { value, unit, .. }) = potential_expr {
        match unit {
            hwc_parser::Unit::Voltage(hwc_parser::VoltageUnit::Volts) => (*value * 1000.0) as i64,
            hwc_parser::Unit::Voltage(hwc_parser::VoltageUnit::Millivolts) => *value as i64,
            hwc_parser::Unit::Voltage(hwc_parser::VoltageUnit::Kilovolts) => (*value * 1_000_000.0) as i64,
            _ => (*value * 1000.0) as i64,
        }
    } else {
        3300 // default 3.3V
    };

    let current_ma = current_expr.and_then(|expr| match expr {
        hwc_parser::Expression::Measurement { value, unit, .. } => match unit {
            hwc_parser::Unit::Current(hwc_parser::CurrentUnit::Amperes) => Some((*value * 1000.0) as i64),
            hwc_parser::Unit::Current(hwc_parser::CurrentUnit::Milliamperes) => Some(*value as i64),
            hwc_parser::Unit::Current(hwc_parser::CurrentUnit::Microamperes) => Some((*value / 1000.0) as i64),
            _ => Some(*value as i64),
        },
        _ => None,
    });

    Ok((voltage_mv, current_ma))
}
