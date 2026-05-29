//! Impedance determination and calculation.
//!
//! This module handles target impedance determination based on net types
//! and actual impedance calculation from trace geometry.

use hwc_physics::EMAnalyzer;

/// Determine target impedance for a net based on its name and type.
///
/// High-speed signals require controlled impedance to prevent signal integrity issues.
/// This method returns the target impedance that the router should aim for.
///
/// **Note**: This returns a TARGET impedance, not a calculated impedance.
/// The actual impedance will be calculated from trace geometry during DRC.
///
/// # Arguments
/// * `net_name` - Name of the net
///
/// # Returns
/// Target impedance in ohms, or None for nets that don't require impedance control
///
/// # Standard Impedances
/// - 50Ω: Single-ended high-speed signals (SPI, UART, general RF)
/// - 90Ω: USB differential pairs (USB 2.0, USB 3.0)
/// - 100Ω: Ethernet differential pairs (10/100/1000 Mbps)
/// - 75Ω: Video signals (HDMI, DisplayPort)
pub fn determine_target_impedance(net_name: &str) -> Option<f64> {
    let name_upper = net_name.to_uppercase();

    // USB differential pairs (90Ω differential, ~45Ω single-ended)
    if name_upper.contains("USB")
        || name_upper.contains("DP")
        || name_upper.contains("DM")
        || name_upper.contains("D+")
        || name_upper.contains("D-")
    {
        return Some(90.0);
    }

    // Ethernet differential pairs (100Ω)
    if name_upper.contains("ETH")
        || name_upper.contains("ETHERNET")
        || name_upper.contains("MDI")
        || name_upper.contains("RJ45")
    {
        return Some(100.0);
    }

    // Video signals (75Ω)
    if name_upper.contains("HDMI")
        || name_upper.contains("DISPLAYPORT")
        || name_upper.contains("VIDEO")
    {
        return Some(75.0);
    }

    // High-speed clocks and data (50Ω)
    if name_upper.contains("CLK")
        || name_upper.contains("CLOCK")
        || name_upper.contains("MISO")
        || name_upper.contains("MOSI")
        || name_upper.contains("SCK")
        || name_upper.contains("SDA")
        || name_upper.contains("SCL")
    {
        return Some(50.0);
    }

    // RF signals (50Ω)
    if name_upper.contains("RF") || name_upper.contains("ANT") || name_upper.contains("ANTENNA") {
        return Some(50.0);
    }

    // Default: no impedance control for regular signals
    None
}

/// Calculate actual trace impedance based on geometry and material properties
///
/// Uses microstrip impedance formula to calculate the characteristic impedance
/// of a trace given its physical dimensions and the dielectric properties.
///
/// # Arguments
/// * `trace_width_nm` - Trace width in nanometers
/// * `copper_thickness_nm` - Copper thickness in nanometers
/// * `dielectric_height_nm` - Height above ground plane in nanometers
/// * `relative_permittivity` - Dielectric constant (εr) of the substrate
///
/// # Returns
/// Characteristic impedance in ohms
///
/// # Formula
/// Z₀ ≈ 87/√(εr+1.41) × ln(5.98h/(0.8w+t))
///
/// This is used during constraint generation to verify that the calculated
/// trace width will achieve the target impedance for high-speed signals.
pub fn calculate_trace_impedance(
    trace_width_nm: i64,
    copper_thickness_nm: i64,
    dielectric_height_nm: i64,
    relative_permittivity: f64,
) -> f64 {
    let analyzer = EMAnalyzer::new();
    analyzer.calculate_microstrip_impedance(
        trace_width_nm,
        copper_thickness_nm,
        dielectric_height_nm,
        relative_permittivity,
    )
}
