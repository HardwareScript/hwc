// Hardware Script Parser Error Codes
//
// Format: S[Digit][Digit] (Syntax errors)
// S = Syntax (parser/lexer errors)
//
// Inspired by DSTV satellite TV error codes (E16, E48) that became
// part of everyday language. Users say "I'm getting S22" instead of
// reading the full error text.

/// Syntax error codes for the Hardware Script parser.
///
/// These 3-character codes are designed to be:
/// - Speakable: "I'm getting S-twenty-two"
/// - Memorable: Short enough to remember and share
/// - Searchable: "Hardware Script S22" finds documentation
/// - Universal: Becomes the vocabulary of the community
pub mod syntax {
    // S10-S19: Structure errors (missing colons, wrong indentation)
    pub const MISSING_COLON: &str = "S11";
    pub const UNEXPECTED_INDENT: &str = "S12";
    pub const MISSING_CLOSING_BRACKET: &str = "S13";
    pub const UNEXPECTED_TOKEN: &str = "S14";
    pub const UNTERMINATED_STRING: &str = "S15";

    // S20-S29: Value errors (invalid coordinates, unknown units)
    pub const INVALID_COORDINATE: &str = "S21";
    pub const UNKNOWN_UNIT: &str = "S22";
    pub const INVALID_NUMBER: &str = "S23";
    pub const COORDINATE_OUT_OF_ORDER: &str = "S24";

    // S30-S39: Keyword errors (typos in keywords)
    pub const UNKNOWN_KEYWORD: &str = "S31";
    pub const KEYWORD_TYPO: &str = "S32";

    // S40-S49: Import/module errors (syntax-level import issues)
    pub const INVALID_IMPORT_SYNTAX: &str = "S41";
    pub const INVALID_MODULE_PATH: &str = "S42";
}
