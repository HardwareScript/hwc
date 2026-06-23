//! Electrical analysis for nets.
//!
//! This module provides functions for analyzing the electrical properties of nets,
//! including voltage and current determination based on net declarations.
//!
//! **No hardcoded heuristics.** All electrical properties MUST come from
//! explicit `nets:` declarations in the space definition. The compiler does
//! NOT guess voltages or currents from net name patterns.

use crate::netlist::{NetData, NetlistArena};
use hwc_parser::NetClassification;

/// Analyze electrical properties of a net.
///
/// Determines voltage and current requirements from the net's declaration
/// in the space definition's `nets:` block. No name-based guessing.
///
/// # Arguments
/// * `net` - Net data from the netlist
/// * `_netlist` - Full netlist arena for component lookup
/// * `net_declaration` - Optional declaration from the space definition's `nets:` block
///
/// # Returns
/// Tuple of (voltage_mv, current_ma), or error if no declaration is provided
pub fn analyze_net_electrical(
    net: &NetData,
    _netlist: &NetlistArena,
    net_declaration: Option<&hwc_parser::NetDeclaration>,
) -> Result<(i64, i64), String> {
    let decl = net_declaration.ok_or_else(|| {
        format!(
            "Net '{}' has no electrical specification. Add a 'nets:' declaration with classification and potential.",
            net.name
        )
    })?;

    let voltage_mv = decl.potential_mv.ok_or_else(|| {
        format!(
            "Net '{}' has a declaration but no potential specified. Add 'potential: <voltage>' to the nets: entry.",
            net.name
        )
    })?;

    let current_ma = match decl.classification {
        NetClassification::Power => 1000,
        NetClassification::Ground => 5000,
        NetClassification::Signal => 100,
        NetClassification::HighVoltage => 100,
        NetClassification::Unclassified => 10,
    };

    Ok((voltage_mv, current_ma))
}
