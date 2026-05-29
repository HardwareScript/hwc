// Hardware Script Engine Error Codes
//
// Format: R[Digit][Digit] (Routing errors), P[Digit][Digit] (Physics errors)
// R = Routing & Engine (physical placement failed)
// P = Physics (laws of physics violated)

/// Routing and engine error codes for Hardware Script.
///
/// These 3-character codes are designed to be:
/// - Speakable: "I'm getting R-twelve"
/// - Memorable: Short enough to remember and share
/// - Searchable: "Hardware Script R12" finds documentation
/// - Universal: Becomes the vocabulary of the community
pub mod routing {
    // R10-R19: Placement errors (collisions, out of bounds)
    pub const OUT_OF_BOUNDS: &str = "R11";
    pub const COMPONENT_COLLISION: &str = "R12";
    pub const TOO_CLOSE_TO_EDGE: &str = "R13";
    pub const COMPONENT_OVERLAPS_SUBSTRATE: &str = "R14";

    // R20-R29: Routing errors (no path found, trace overlap)
    pub const NO_ROUTE_FOUND: &str = "R21";
    pub const TRACE_OVERLAP: &str = "R22";
    pub const WAYPOINT_UNREACHABLE: &str = "R23";
    pub const EMPTY_WAYPOINTS: &str = "R24";

    // R30-R39: Geometry errors (invalid angles, impossible vias)
    pub const INVALID_TURN: &str = "R31";
    pub const DIAGONAL_VIA: &str = "R32";
    pub const VIA_ON_WRONG_LAYER: &str = "R33";
}

/// Physics error codes for Hardware Script.
///
/// P16 (Dielectric Breakdown) is THE FAMOUS ONE - like DSTV's E16.
pub mod physics {
    // P10-P19: Clearance/voltage errors (dielectric breakdown)
    pub const DIELECTRIC_BREAKDOWN: &str = "P16"; // THE FAMOUS ONE
    pub const VOLTAGE_EXCEEDS_RATING: &str = "P17";
    pub const CLEARANCE_TOO_SMALL: &str = "P18";

    // P20-P29: Thermal/current errors (overheating, trace too thin)
    pub const VOLTAGE_DROP_TOO_HIGH: &str = "P20";
    pub const TRACE_TOO_THIN: &str = "P21";
    pub const COMPONENT_OVERHEATING: &str = "P22";
    pub const RESISTANCE_TOO_HIGH: &str = "P23";
    pub const TEMPERATURE_RISE_EXCEEDS_LIMIT: &str = "P24";
    pub const THERMAL_CLUSTERING: &str = "P25";
    pub const VIA_CURRENT_EXCEEDED: &str = "P26";

    // P30-P39: Signal integrity errors (impedance, crosstalk)
    pub const IMPEDANCE_MISMATCH: &str = "P31";
    pub const CROSSTALK_RISK: &str = "P32";
    pub const STUB_TOO_LONG: &str = "P33";
    pub const SIGNAL_INTEGRITY_VIOLATION: &str = "P34";
}
