//! Token types for Hardware Script language
//!
//! Based on v0.1.6 syntax unification specification.
//! See `grammar/hardware.grammar` for complete syntax rules.
//!
//! v0.1.6 Changes:
//! - Removed `define` keyword - type keywords are now first-class
//! - Example: `component Resistor:` not `define component "Resistor":`

use logos::Logos;
use std::fmt;

use super::parsers::*;
use super::units::Measurement;

// Helper functions for parsing integer literals with different bases
fn parse_any_integer(lex: &mut logos::Lexer<Token>) -> Option<i64> {
    let slice = lex.slice();

    // For decimal integers, let Rust's native parser handle the sign
    // This correctly handles i64::MIN (-9223372036854775808)
    if !slice.contains("0x")
        && !slice.contains("0X")
        && !slice.contains("0b")
        && !slice.contains("0B")
        && !slice.contains("0o")
        && !slice.contains("0O")
    {
        return slice.parse::<i64>().ok();
    }

    // For hex/binary/octal, we need to handle the sign manually
    // because from_str_radix doesn't accept signs
    let (sign, rest) = if let Some(stripped) = slice.strip_prefix('+') {
        (1i64, stripped)
    } else if let Some(stripped) = slice.strip_prefix('-') {
        (-1i64, stripped)
    } else {
        (1i64, slice)
    };

    // Parse based on prefix
    let value = if rest.starts_with("0x") || rest.starts_with("0X") {
        i64::from_str_radix(&rest[2..], 16).ok()?
    } else if rest.starts_with("0b") || rest.starts_with("0B") {
        i64::from_str_radix(&rest[2..], 2).ok()?
    } else if rest.starts_with("0o") || rest.starts_with("0O") {
        i64::from_str_radix(&rest[2..], 8).ok()?
    } else {
        rest.parse::<i64>().ok()?
    };

    Some(sign * value)
}

/// Token types for Hardware Script language
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")] // Skip spaces and tabs (but not newlines - we need them for indentation)
pub enum Token {
    // ========================================================================
    // ACTION VERBS - Primary commands
    // ========================================================================
    #[token("import")]
    Import,

    #[token("add")]
    Add,

    #[token("route")]
    Route,

    #[token("expose")]
    Expose,

    // ========================================================================
    // CONNECTORS & PREPOSITIONS - Relational keywords
    // ========================================================================
    #[token("from")]
    From,

    #[token("named")]
    Named,

    #[token("at")]
    At,

    #[token("on")]
    On,

    #[token("rotated")]
    Rotated,

    #[token("to")]
    To,

    #[token("by")]
    By,

    #[token("spanning")]
    Spanning,

    #[token("as")]
    As,

    #[token("implements")]
    Implements,

    #[token("bridge")]
    Bridge,

    // v0.1.7: Port escape keywords
    #[token("exit")]
    Exit,

    #[token("enter")]
    Enter,

    // v0.1.9: Middle-level relational constraint keywords
    #[token("align")]
    Align,

    #[token("with")]
    With,

    #[token("above")]
    Above,

    #[token("below")]
    Below,

    #[token("right_of")]
    RightOf,

    #[token("left_of")]
    LeftOf,

    // ========================================================================
    // BLOCK KEYS - Property names (strictly lowercase)
    // ========================================================================
    #[token("dimensions")]
    Dimensions,

    #[token("grid")]
    Grid,

    #[token("resolution")]
    Resolution,

    #[token("path")]
    Path,

    #[token("origin")]
    Origin,

    // ========================================================================
    // ORIGIN POINTS - Shorthand coordinate system origins (strictly lowercase)
    // ========================================================================
    #[token("tl")]
    TopLeft,

    #[token("bl")]
    BottomLeft,

    #[token("tr")]
    TopRight,

    #[token("br")]
    BottomRight,

    // ========================================================================
    // Z-AXIS ORIGIN DIRECTION - Vertical coordinate system direction
    // NOTE: Removed 't' and 'b' as keywords - they conflict with logic synthesis
    // variables. Parse them as identifiers in coordinate context instead.
    // ========================================================================
    // (tokens removed - parse as identifiers contextually)

    // ========================================================================
    // TYPE KEYWORDS (strictly lowercase) - v0.1.4 define blocks
    // ========================================================================
    #[token("space")]
    Space,

    #[token("material")]
    Material,

    #[token("profile")]
    Profile,

    #[token("component")]
    Component,

    #[token("module")]
    Module,

    #[token("mechanical")]
    Mechanical,

    #[token("interface")]
    Interface,

    #[token("test")]
    Test,

    #[token("substrate")]
    Substrate,

    #[token("shape")]
    Shape,

    #[token("unit")]
    Unit,

    #[token("device")]
    Device,

    #[token("signal_group")]
    SignalGroup,

    #[token("net_type")]
    NetType,

    // Note: 'pattern' and 'strategy' are handled as identifiers in most contexts
    // and only recognized as definition types after 'define' keyword

    // ========================================================================
    // LOGIC SYNTHESIS KEYWORDS (v0.4.0)
    // ========================================================================
    #[token("logic")]
    Logic,

    #[token("enum")]
    Enum,

    #[token("struct")]
    Struct,

    #[token("let")]
    Let,

    #[token("mut")]
    Mut,

    #[token("match")]
    Match,

    #[token("reg")]
    RegisterInit,

    #[token("true")]
    True,

    #[token("false")]
    False,

    // ========================================================================
    // CONSTANT DEFINITIONS (v0.1.6) - For math.hw primitives
    // ========================================================================
    #[token("const")]
    Const,

    // ========================================================================
    // LOGIC OPERATORS (v0.1.6) - Word-form alternatives to symbols
    // ========================================================================
    #[token("and")]
    And,

    #[token("or")]
    Or,

    #[token("not")]
    Not,

    #[token("xor")]
    Xor,

    #[token("mod")]
    Mod,

    // ========================================================================
    // PARAMETRIC LOOP KEYWORDS (v0.1.6 Sprint 3.4)
    // ========================================================================
    /// Reference to the previous iteration's component in a for loop
    /// Used for relative positioning: `after: last.right + 1mm`
    #[token("last")]
    Last,

    // ========================================================================
    // SPACE STATEMENT TYPES (for add statements)
    // ========================================================================
    #[token("pour")]
    Pour,

    #[token("plane")]
    Plane,

    #[token("polygon")]
    Polygon,

    #[token("contact")]
    Contact,

    // ========================================================================
    // PROPERTY KEYWORDS - REMOVED IN v0.1.6
    // Property names like 'tolerance', 'trace', 'via', etc. are now parsed
    // as regular identifiers, not special keywords. This simplifies the lexer
    // and makes property blocks more flexible.
    // ========================================================================

    // NOTE: 'r' removed as keyword - conflicts with logic synthesis variables
    // Parse it as an identifier in rotation context instead
    // (token removed - parse as identifier contextually)

    // ========================================================================
    // CONTROL FLOW KEYWORDS (for comptime evaluation in modules)
    // ========================================================================
    #[token("for")]
    For,

    #[token("in")]
    In,

    #[token("if")]
    If,

    #[token("then")]
    Then,

    #[token("else")]
    Else,

    // ========================================================================
    // PIN DIRECTION KEYWORDS - REMOVED (v0.1.6 Context-Aware Parsing)
    // These are now parsed as regular identifiers and recognized as property
    // names in context by the module parser. This prevents keyword pollution
    // and allows these words to be used freely elsewhere (e.g., in units.hw).
    // See: ROADMAP/v0.1.6/CONTEXT-AWARE-PARSING.md
    // ========================================================================

    // ========================================================================
    // PUNCTUATION
    // ========================================================================
    #[token(":")]
    Colon,

    #[token("[")]
    OpenBracket,

    #[token("]")]
    CloseBracket,

    #[token("(")]
    OpenParen,

    #[token(")")]
    CloseParen,

    #[token("-")]
    Hyphen,

    #[token(".")]
    Dot,

    #[token(",")]
    Comma,

    /// Import path: @org/path/to/module
    /// Pattern: @ followed by alphanumeric/underscore segments separated by /
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*(/[a-zA-Z_][a-zA-Z0-9_]*)*", |lex| lex.slice().to_string())]
    ImportPath(String),

    #[token("@")]
    AtSymbol,

    #[token("/")]
    Slash,

    #[token("=")]
    Equals,

    #[token("<")]
    LessThan,

    #[token(">")]
    GreaterThan,

    #[token("..")]
    Range,

    #[token("!=")]
    NotEquals,

    #[token("+")]
    Plus,

    #[token("*")]
    Asterisk,

    #[token("%")]
    Percent,

    #[token("&")]
    Ampersand,

    #[token("|")]
    Pipe,

    #[token("~")]
    Tilde,

    #[token("!")]
    Exclamation,

    #[token("<<")]
    ShiftLeft,

    #[token(">>")]
    ShiftRight,

    #[token("<=")]
    LessThanOrEqual,

    #[token(">=")]
    GreaterThanOrEqual,

    #[token("{")]
    OpenBrace,

    #[token("}")]
    CloseBrace,

    // ========================================================================
    // LITERALS
    // ========================================================================
    /// Identifiers: PascalCase, snake_case, or camelCase
    /// Pattern: starts with letter, contains letters, digits, underscores
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),

    /// Floating point numbers with optional scientific notation (standalone only)
    /// Must come before Integer to match decimals first
    /// Priority 18 ensures Float matches before Measurement (priority 16) for scientific notation
    ///
    /// **CRITICAL FIX (Sprint 3.9 - "Lexer Greed" Bug)**:
    /// Removed [+-]? prefix from number patterns. Signs are now separate operator tokens.
    ///
    /// **Why**: In `i+1`, the old pattern `[+-]?\d+` would match `+1` as a single Integer token,
    /// consuming the `+` operator. This caused `Carry[i+1]` to become `Carry[i1]` → `Carry[11]`.
    ///
    /// **Physical Reality Rule**: A sign is an **operator** (instruction), not part of the **value** (atom).
    /// The parser handles unary operators (e.g., `-10mm`) by looking for Plus/Hyphen before the number.
    ///
    /// Supports: 3.14, 1e3, 1e+3, 1e-3, 1.2e-5 (note: signs in exponents are still allowed)
    #[regex(r"\d+\.\d+([eE][+-]?\d+)?", priority = 18, callback = |lex| lex.slice().parse::<f64>().ok())]
    #[regex(r"\d+[eE][+-]?\d+", priority = 18, callback = |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),

    /// Integer numbers - standalone only
    /// Supports decimal (42), hexadecimal (0xFF), binary (0b1010), octal (0o77)
    ///
    /// **CRITICAL FIX (Sprint 3.9)**: Removed [+-]? prefix - signs are now separate tokens
    /// Priority 17 ensures hex/binary/octal are matched before Measurement (priority 16)
    #[regex(r"0[xX][0-9a-fA-F]+", parse_any_integer, priority = 17)]
    #[regex(r"0[bB][01]+", parse_any_integer, priority = 17)]
    #[regex(r"0[oO][0-7]+", parse_any_integer, priority = 17)]
    #[regex(r"[0-9]+", parse_any_integer)]
    Integer(i64),

    /// String literals (double-quoted)
    /// Priority 20 ensures strings are matched before punctuation tokens (Comma, Dot, etc.)
    /// Regex: "(?:[^"\\]|\\.)*" matches either "any char except quote/backslash" OR "backslash + any char"
    /// This properly handles escaped quotes: "He said \"hello\""
    #[regex(r#""(?:[^"\\]|\\.)*""#, priority = 20, callback = |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()  // Remove quotes
    })]
    String(String),

    // ========================================================================
    // NATIVE SI UNIT MEASUREMENTS (v0.1.4)
    // Pattern: NUMBER + UNIT (e.g., 254µm, 4.7kΩ, 100nF, 1%, 100ppm)
    // Generic measurement parser - handles ALL units including unknown ones
    // Known physics units (mm, V, A, Ω, etc.) are parsed to specific enums
    // Unknown units (%, ppm, mAh, dBm) are stored as Custom(String)
    // No space allowed between number and unit for unambiguous parsing
    // Note: Negative measurements (-5V) are handled by parser (Hyphen + Measurement)
    // ========================================================================
    /// Generic measurement: NUMBER followed immediately by UNIT_STRING (NO SPACES)
    /// Matches: 10mm, 3.3V, 4.7kΩ, 1%, 10Å, 5μA, 1.2M⊙, 8960kg/m³, 401W/(m·K)
    ///
    /// THE LEXER IS TOTALLY DUMB - It accepts ANY Unicode letter or symbol after a number.
    /// The SymbolTable is responsible for validating if the unit is defined in stdlib or imports.
    /// This allows users to define custom units (Å, ℓ, ⊙, etc.) without compiler updates.
    ///
    /// Regex breakdown:
    /// - [+-]?\d+\.?\d*([eE][+-]?\d+)? = Number with optional scientific notation
    /// - [\p{L}\p{S}\p{M}µΩ°%·/²³] = First char: ANY Unicode letter, symbol, or mark
    /// - [\p{L}\p{S}\p{M}0-9µΩ°%·/²³/]* = Rest: letters, symbols, digits, slash (NO PARENS)
    /// - (?:\([\p{L}\p{S}\p{M}0-9µΩ°%·/²³/]+\))* = Optional balanced parens for complex units
    ///
    /// \p{L} = Unicode letters (Latin, Greek, Cyrillic, etc.)
    /// \p{S} = Unicode symbols (⊙, ℓ, ∞, etc.)
    /// \p{M} = Unicode marks (combining diacritics)
    ///
    /// CRITICAL FIX: Removed \(\) from the main character class to prevent greedy consumption
    /// of closing parentheses from shape definitions like Rectangle(2mm, 2mm, 0.5mm)
    /// Parentheses are ONLY allowed in balanced pairs via the final group
    ///
    /// **CRITICAL FIX (Sprint 3.9)**: Removed [+-]? prefix - signs are now separate tokens
    /// Priority 16 ensures Measurement matches before Integer (priority 15)
    #[regex(r"\d+\.?\d*([eE][+-]?\d+)?[\p{L}\p{S}\p{M}_µΩ°%·/²³][\p{L}\p{S}\p{M}0-9_µΩ°%·/²³/]*(?:\([\p{L}\p{S}\p{M}0-9µΩ°%·/²³/]+\))*", priority = 16, callback = parse_generic_measurement)]
    Measurement(Measurement),

    // ========================================================================
    // COMMENTS
    // ========================================================================
    /// Multi-line documentation block: ##[ ... ]##
    /// These are KEPT in the AST for documentation generation
    /// Whitespace after ##[ is optional
    #[regex(r"##\[", priority = 3, callback = parse_doc_block)]
    DocBlock(String),

    /// Multi-line comment block: #[ ... ]#
    /// These are KEPT in the AST for documentation generation
    /// Whitespace after #[ is optional
    #[regex(r"#\[", priority = 2, callback = parse_block_comment)]
    BlockComment(String),

    /// Documentation comment (section header) - ## followed by anything except [ or newline
    /// These are KEPT in the AST for documentation generation
    /// CRITICAL: [^\[\n]* ensures we DON'T consume the newline, leaving it for the parser
    /// Now allows empty ## lines (useful for visual separation in doc blocks)
    #[regex(r"##(?:[^\[\n][^\n]*)?", priority = 1, callback = |lex| Some(lex.slice()[2..].trim().to_string()))]
    DocComment(String),

    /// Single-line comment - # followed by anything except # or [ or newline
    /// THE TRUE "TRASH CAN" PATTERN: These are COMPLETELY SKIPPED by the lexer
    /// The parser NEVER sees them - they are invisible, just like in Rust and Elixir
    /// Match: # followed by anything that isn't # or [ or newline, then SKIP IT
    /// The newline is left untouched for Token::Newline to catch
    /// v0.1.7: Allow empty comments (just '#') by making the content optional
    #[regex(r"#(?:[^#\[\n][^\n]*)?", logos::skip)]
    // ========================================================================
    // WHITESPACE & STRUCTURE
    // ========================================================================
    /// Newline (needed for indentation tracking)
    #[token("\n")]
    Newline,

    /// Indentation increase
    Indent,

    /// Indentation decrease
    Dedent,

    /// End of file
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Translate compiler-speak to human-speak
            Token::Identifier(s) => write!(f, "the name '{}'", s),
            Token::Integer(n) => write!(f, "the number {}", n),
            Token::Float(n) => write!(f, "the number {}", n),
            Token::String(s) => write!(f, "the text \"{}\"", s),

            // Format punctuation conversationally
            Token::OpenBracket => write!(f, "a square bracket '['"),
            Token::CloseBracket => write!(f, "a square bracket ']'"),
            Token::OpenParen => write!(f, "a parenthesis '('"),
            Token::CloseParen => write!(f, "a parenthesis ')'"),
            Token::Colon => write!(f, "a colon ':'"),
            Token::Comma => write!(f, "a comma ','"),
            Token::Dot => write!(f, "a dot '.'"),
            Token::Hyphen => write!(f, "a hyphen '-'"),
            Token::ImportPath(path) => write!(f, "import path '{}'", path),
            Token::AtSymbol => write!(f, "an @ symbol"),
            Token::Slash => write!(f, "a slash '/'"),
            Token::Equals => write!(f, "an equals sign '='"),
            Token::LessThan => write!(f, "a less-than sign '<'"),
            Token::GreaterThan => write!(f, "a greater-than sign '>'"),

            // Format keywords clearly
            Token::Import => write!(f, "the 'import' keyword"),
            Token::Add => write!(f, "the 'add' keyword"),
            Token::Route => write!(f, "the 'route' keyword"),
            Token::Expose => write!(f, "the 'expose' keyword"),
            Token::From => write!(f, "the 'from' keyword"),
            Token::Named => write!(f, "the 'named' keyword"),
            Token::At => write!(f, "the 'at' keyword"),
            Token::On => write!(f, "the 'on' keyword"),
            Token::Rotated => write!(f, "the 'rotated' keyword"),
            Token::To => write!(f, "the 'to' keyword"),
            Token::By => write!(f, "the 'by' keyword"),
            Token::Spanning => write!(f, "the 'spanning' keyword"),
            Token::As => write!(f, "the 'as' keyword"),
            Token::Implements => write!(f, "the 'implements' keyword"),
            Token::Bridge => write!(f, "the 'bridge' keyword"),
            Token::Exit => write!(f, "the 'exit' keyword"),
            Token::Enter => write!(f, "the 'enter' keyword"),
            Token::Align => write!(f, "the 'align' keyword"),
            Token::With => write!(f, "the 'with' keyword"),
            Token::Above => write!(f, "the 'above' keyword"),
            Token::Below => write!(f, "the 'below' keyword"),
            Token::RightOf => write!(f, "the 'right_of' keyword"),
            Token::LeftOf => write!(f, "the 'left_of' keyword"),
            Token::Dimensions => write!(f, "the 'dimensions' keyword"),
            Token::Grid => write!(f, "the 'grid' keyword"),
            Token::Resolution => write!(f, "the 'resolution' keyword"),
            Token::Path => write!(f, "the 'path' keyword"),
            Token::Origin => write!(f, "the 'origin' keyword"),
            Token::TopLeft => write!(f, "'tl' (top-left origin)"),
            Token::BottomLeft => write!(f, "'bl' (bottom-left origin)"),
            Token::TopRight => write!(f, "'tr' (top-right origin)"),
            Token::BottomRight => write!(f, "'br' (bottom-right origin)"),
            Token::Space => write!(f, "the 'space' keyword"),
            Token::Material => write!(f, "the 'material' keyword"),
            Token::Profile => write!(f, "the 'profile' keyword"),
            Token::Component => write!(f, "the 'component' keyword"),
            Token::Module => write!(f, "the 'module' keyword"),
            Token::Mechanical => write!(f, "the 'mechanical' keyword"),
            Token::Interface => write!(f, "the 'interface' keyword"),
            Token::Test => write!(f, "the 'test' keyword"),
            Token::Substrate => write!(f, "the 'substrate' keyword"),
            Token::Shape => write!(f, "the 'shape' keyword"),
            Token::Unit => write!(f, "the 'unit' keyword"),
            Token::Device => write!(f, "the 'device' keyword"),
            Token::SignalGroup => write!(f, "the 'signal_group' keyword"),
            Token::NetType => write!(f, "the 'net_type' keyword"),
            Token::Logic => write!(f, "the 'logic' keyword"),
            Token::Enum => write!(f, "the 'enum' keyword"),
            Token::Struct => write!(f, "the 'struct' keyword"),
            Token::Let => write!(f, "the 'let' keyword"),
            Token::Mut => write!(f, "the 'mut' keyword"),
            Token::Match => write!(f, "the 'match' keyword"),
            Token::RegisterInit => write!(f, "the 'reg' keyword"),
            Token::True => write!(f, "the 'true' keyword"),
            Token::False => write!(f, "the 'false' keyword"),
            Token::Const => write!(f, "the 'const' keyword"),
            Token::And => write!(f, "the 'and' keyword"),
            Token::Or => write!(f, "the 'or' keyword"),
            Token::Not => write!(f, "the 'not' keyword"),
            Token::Xor => write!(f, "the 'xor' keyword"),
            Token::Mod => write!(f, "the 'mod' keyword"),
            Token::Pour => write!(f, "the 'pour' keyword"),
            Token::Plane => write!(f, "the 'plane' keyword"),
            Token::Polygon => write!(f, "the 'polygon' keyword"),
            Token::Contact => write!(f, "the 'contact' keyword"),
            Token::For => write!(f, "the 'for' keyword"),
            Token::In => write!(f, "the 'in' keyword"),
            Token::If => write!(f, "the 'if' keyword"),
            Token::Then => write!(f, "the 'then' keyword"),
            Token::Else => write!(f, "the 'else' keyword"),
            Token::Last => write!(f, "the 'last' keyword"),
            Token::Range => write!(f, "a range operator '..'"),
            Token::NotEquals => write!(f, "a not-equals operator '!='"),
            Token::Plus => write!(f, "a plus sign '+'"),
            Token::Asterisk => write!(f, "an asterisk '*'"),
            Token::Percent => write!(f, "a percent sign '%'"),
            Token::Ampersand => write!(f, "an ampersand '&'"),
            Token::Pipe => write!(f, "a pipe '|'"),
            Token::Tilde => write!(f, "a tilde '~'"),
            Token::Exclamation => write!(f, "an exclamation mark '!'"),
            Token::ShiftLeft => write!(f, "a left shift operator '<<'"),
            Token::ShiftRight => write!(f, "a right shift operator '>>'"),
            Token::LessThanOrEqual => write!(f, "a less-than-or-equal operator '<='"),
            Token::GreaterThanOrEqual => write!(f, "a greater-than-or-equal operator '>='"),
            Token::OpenBrace => write!(f, "an open brace '{{'"),
            Token::CloseBrace => write!(f, "a close brace '}}'"),

            // Native SI unit measurements
            Token::Measurement(m) => write!(f, "{}", m),

            // Comments and whitespace
            Token::DocBlock(_) => write!(f, "a documentation block"),
            Token::BlockComment(_) => write!(f, "a comment block"),
            Token::DocComment(_) => write!(f, "a documentation comment"),
            Token::Newline => write!(f, "newline"),
            Token::Indent => write!(f, "indentation"),
            Token::Dedent => write!(f, "dedentation"),
            Token::Eof => write!(f, "end of file"),
        }
    }
}
