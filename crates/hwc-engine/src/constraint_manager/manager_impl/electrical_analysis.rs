//! Electrical analysis for nets.
//!
//! This module provides functions for analyzing the electrical properties of nets,
//! including voltage and current determination based on net names and component connections.

use crate::netlist::{NetData, NetlistArena};

/// Analyze electrical properties of a net.
///
/// Determines voltage and current requirements for a net based on
/// connected components and their electrical specifications.
///
/// **Current Implementation**: Uses heuristics based on net name patterns
/// and connected component types. Future versions will use full circuit
/// simulation.
///
/// # Arguments
/// * `net` - Net data from the netlist
/// * `netlist` - Full netlist arena for component lookup
///
/// # Returns
/// Tuple of (voltage_mv, current_ma), or error if analysis fails
///
/// # Heuristics
/// - Power nets (VCC, VDD, 3V3, 5V, etc.): Extract voltage from name
/// - Ground nets (GND, VSS): 0V, high current capacity
/// - Signal nets: Default to 3.3V, low current
/// - High-speed nets (CLK, DATA): Default to 3.3V, medium current
pub fn analyze_net_electrical(
    net: &NetData,
    _netlist: &NetlistArena,
) -> Result<(i64, i64), String> {
    let name_upper = net.name.to_uppercase();

    // Power net detection
    if name_upper.contains("VCC")
        || name_upper.contains("VDD")
        || name_upper.contains("VBAT")
        || name_upper.contains("POWER")
    {
        // Try to extract voltage from name (e.g., "VCC_3V3" → 3300mV)
        let voltage_mv = extract_voltage_from_name(&net.name).unwrap_or(5000); // Default 5V
        let current_ma = 1000; // Default 1A for power nets
        return Ok((voltage_mv, current_ma));
    }

    // Ground net detection
    if name_upper.contains("GND") || name_upper.contains("VSS") || name_upper.contains("GROUND") {
        return Ok((0, 5000)); // 0V, 5A capacity for ground
    }

    // High voltage AC detection
    if name_upper.contains("AC_LINE")
        || name_upper.contains("MAINS")
        || name_upper.contains("120V")
        || name_upper.contains("240V")
    {
        let voltage_mv = if name_upper.contains("240V") {
            240_000
        } else {
            120_000
        };
        return Ok((voltage_mv, 1000)); // 1A for AC lines
    }

    // High-speed signal detection
    if name_upper.contains("CLK")
        || name_upper.contains("CLOCK")
        || name_upper.contains("DATA")
        || name_upper.contains("USB")
    {
        return Ok((3300, 100)); // 3.3V, 100mA for high-speed signals
    }

    // Default signal net
    Ok((3300, 10)) // 3.3V, 10mA for generic signals
}

/// Extract voltage value from net name.
///
/// Parses common voltage naming patterns:
/// - "VCC_3V3" → 3300mV
/// - "VDD_5V" → 5000mV
/// - "VBAT_12V" → 12000mV
/// - "3V3_RAIL" → 3300mV
///
/// # Arguments
/// * `name` - Net name to parse
///
/// # Returns
/// Voltage in millivolts, or None if no voltage pattern found
pub fn extract_voltage_from_name(name: &str) -> Option<i64> {
    let name_upper = name.to_uppercase();

    // Pattern: "3V3" → 3.3V
    if let Some(pos) = name_upper.find("V3") {
        if pos > 0 {
            let before = &name_upper[pos - 1..pos];
            if let Ok(volts) = before.parse::<i64>() {
                return Some(volts * 1000 + 300); // e.g., "3V3" → 3300mV
            }
        }
    }

    // Pattern: "5V", "12V" → extract number before last V
    // Use rfind to get the LAST 'V' in the string (avoids matching VCC, VDD, etc.)
    if let Some(v_pos) = name_upper.rfind('V') {
        // Look backwards from V to find the start of the number
        let before_v = &name_upper[..v_pos];

        // Find where the number starts (skip non-digits from the end)
        let num_end = before_v.len();
        let mut num_start = num_end;

        // Scan backwards to find continuous digits
        for (i, ch) in before_v.char_indices().rev() {
            if ch.is_ascii_digit() {
                num_start = i;
            } else {
                break;
            }
        }

        if num_start < num_end {
            let num_str = &before_v[num_start..num_end];
            if let Ok(volts) = num_str.parse::<i64>() {
                return Some(volts * 1000);
            }
        }
    }

    None
}
