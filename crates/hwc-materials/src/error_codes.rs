// Hardware Script Materials Error Codes
//
// Format: M[Digit][Digit] (Manufacturing/Materials errors)
// M = Manufacturing (factory can't build it)

/// Manufacturing and materials error codes for Hardware Script.
///
/// These 3-character codes are designed to be:
/// - Speakable: "I'm getting M-twenty-one"
/// - Memorable: Short enough to remember and share
/// - Searchable: "Hardware Script M21" finds documentation
/// - Universal: Becomes the vocabulary of the community
pub mod manufacturing {
    // M10-M19: Factory limit errors (trace too thin, hole too small)
    pub const TRACE_TOO_THIN_FOR_FAB: &str = "M11";
    pub const VIA_TOO_SMALL: &str = "M12";
    pub const ANNULAR_RING_TOO_SMALL: &str = "M13";
    pub const ASPECT_RATIO_EXCEEDED: &str = "M14";
    pub const PAD_SIZE_TOO_SMALL: &str = "M15";

    // M20-M29: Material errors (material not available)
    pub const MATERIAL_NOT_FOUND: &str = "M21";
    pub const MATERIAL_NOT_AVAILABLE: &str = "M22";
    pub const MATERIAL_INCOMPATIBLE_WITH_PROCESS: &str = "M23";

    // M30-M39: Process errors (incompatible processes)
    pub const INCOMPATIBLE_PROCESS: &str = "M31";
    pub const LAYER_STACK_NOT_SUPPORTED: &str = "M32";
}
