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
    pub const UNEXPECTED_TOKEN: &str = "S14";
    pub const UNEXPECTED_EOF: &str = "S15";
    pub const GENERAL: &str = "S99";

    // S20-S29: Context-aware boundary law errors
    pub const EXPECTED_COLON_IN_PROPERTY: &str = "S20";
    pub const EXPECTED_EQUALS_IN_LOGIC: &str = "S21";
    pub const USES_SINGLE_EQUALS_FOR_COMPARISON: &str = "S22";
    pub const EXPECTED_IDENTIFIER_NOT_STRING: &str = "S23";
    pub const DEFINE_KEYWORD_REMOVED: &str = "S24";
    pub const REGISTER_PRIMITIVE_IS_LOWERCASE: &str = "S25";
    pub const FIELDS_KEYWORD_REMOVED: &str = "S26";
    pub const PERCENT_AS_OPERATOR: &str = "S27";

    // S30-S39: Specific structural errors (broke out from S14)
    pub const EXPECTED_COLON: &str = "S30";
    pub const EXPECTED_QUOTED_STRING: &str = "S31";
    pub const EXPECTED_IDENTIFIER: &str = "S32";
    pub const EXPECTED_EXPRESSION: &str = "S33";
    pub const EXPECTED_NEWLINE: &str = "S34";
    pub const EXPECTED_INDENT: &str = "S35";
    pub const EXPECTED_CLOSING_DELIMITER: &str = "S36";
    pub const EXPECTED_PROPERTY_KEYWORD: &str = "S37";

    // S40-S49: Semantic/validation errors
    pub const UNKNOWN_FIELD: &str = "S40";
    pub const INVALID_SYNTAX: &str = "S41";
    pub const DEPRECATED_SYNTAX: &str = "S42";
    pub const INVALID_EXPRESSION: &str = "S43";
}
