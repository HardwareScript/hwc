// Hardware Script Compiler Error Codes
//
// Format: C[Digit][Digit] (Compiler errors), L[Digit][Digit] (Logic errors)
// C = Compiler (IR & general compilation errors)
// L = Logic (Logic synthesis & hardware design errors)
//
// The syntax is fine, but the logic is flawed.

/// Compiler error codes for Hardware Script IR compilation.
///
/// These 3-character codes are designed to be:
/// - Speakable: "I'm getting C-eleven"
/// - Memorable: Short enough to remember and share
/// - Searchable: "Hardware Script C11" finds documentation
/// - Universal: Becomes the vocabulary of the community
pub mod compiler {
    // C10-C19: Reference errors (component/pin not found)
    pub const COMPONENT_NOT_FOUND: &str = "C11";
    pub const PIN_NOT_FOUND: &str = "C12";
    pub const NET_NAME_CONFLICT: &str = "C13";
    pub const DUPLICATE_COMPONENT_NAME: &str = "C14";

    // C20-C29: Import/dependency errors (circular imports, missing packages)
    pub const PACKAGE_NOT_FOUND: &str = "C21";
    pub const CIRCULAR_DEPENDENCY: &str = "C22";
    pub const VERSION_CONFLICT: &str = "C23";
    pub const IMPORT_PATH_NOT_FOUND: &str = "C24";
    pub const SYMBOL_NOT_FOUND: &str = "C25";
    pub const PRIVATE_SYMBOL_ACCESS: &str = "C26";

    // C30-C39: Space definition errors (multiple spaces, missing dimensions)
    pub const MULTIPLE_SPACES: &str = "C31";
    pub const MISSING_DIMENSIONS: &str = "C32";
    pub const MISSING_GRID: &str = "C33";
    pub const INVALID_DIMENSION_VALUES: &str = "C34";

    // C40-C49: Type/parameter errors (wrong parameter types)
    pub const INVALID_PARAMETER_TYPE: &str = "C41";
    pub const MISSING_REQUIRED_PARAMETER: &str = "C42";

    // C50-C59: Symbol table errors (duplicate definitions, undefined references)
    pub const DUPLICATE_MATERIAL: &str = "C51";
    pub const DUPLICATE_PROFILE: &str = "C52";
    pub const DUPLICATE_COMPONENT: &str = "C53";
    pub const DUPLICATE_MECHANICAL: &str = "C54";
    pub const DUPLICATE_INTERFACE: &str = "C55";
    pub const DUPLICATE_TEST: &str = "C56";
    pub const UNDEFINED_MATERIAL: &str = "C57";
    pub const UNDEFINED_PROFILE: &str = "C58";
    pub const UNDEFINED_COMPONENT: &str = "C59";
}

/// Logic synthesis error codes for Hardware Script logic blocks.
///
/// These codes help hardware engineers debug logic synthesis issues
/// with clear, actionable error messages and examples.
pub mod logic {
    // L01-L09: Variable and wire errors
    pub const UNBOUND_WIRE: &str = "L01";
    pub const WIDTH_MISMATCH: &str = "L02";
    pub const COMBINATIONAL_LOOP: &str = "L03";
    pub const CLOCK_DOMAIN_CROSSING: &str = "L04";
    pub const MULTIPLE_DRIVERS: &str = "L05";
    pub const UNINITIALIZED_REGISTER: &str = "L06";

    // L07-L09: Type system errors
    pub const INVALID_ENUM_VARIANT: &str = "L07";
    pub const STRUCT_FIELD_MISMATCH: &str = "L08";
    pub const TYPE_MISMATCH: &str = "L09";

    // L10-L19: Operator and expression errors
    pub const INVALID_OPERATOR: &str = "L10";
    pub const INVALID_OPERAND_TYPE: &str = "L11";
    pub const DIVISION_BY_ZERO: &str = "L12";

    // L20-L29: Control flow errors
    pub const UNREACHABLE_CODE: &str = "L20";
    pub const MISSING_MATCH_ARM: &str = "L21";
    pub const DUPLICATE_MATCH_ARM: &str = "L22";

    // L30-L39: Register and timing errors
    pub const MISSING_CLOCK_SIGNAL: &str = "L30";
    pub const MISSING_RESET_SIGNAL: &str = "L31";
    pub const INVALID_CLOCK_EDGE: &str = "L32";
}
