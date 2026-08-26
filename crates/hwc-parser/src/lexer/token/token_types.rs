//! HardwareScript v0.3.0 Canonical Token Type Definitions
//!
//! Lexical specification according to HardwareScript v0.3.0 Language Grammar.
//! Significant whitespace / indentation is completely eliminated.
//! Block scoping is strictly delimited by `{` and `}`.

use logos::Logos;
use super::super::parsers::*;
use super::super::units::Measurement;
use super::interpolation::{parse_interpolated_identifier, InterpolatedPart};
use super::number_parsers::parse_any_integer;

/// Token types for HardwareScript v0.3.0
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = ())]
#[logos(skip r"[ \t\r\n\f]+")] // Whitespace is strictly a separator
#[logos(skip r"//[^\n]*")]
#[logos(skip r"#(?:[^#\[\n][^\n]*)?")]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
pub enum Token {
    // ========================================================================
    // 1. CORE COMPTIME & TURING KEYWORDS
    // ========================================================================
    #[token("fn")]
    Fn,

    #[token("let")]
    Let,

    #[token("mut")]
    Mut,

    #[token("const")]
    Const,

    #[token("struct")]
    Struct,

    #[token("enum")]
    Enum,

    #[token("if")]
    If,

    #[token("else")]
    Else,

    #[token("for")]
    For,

    #[token("in")]
    In,

    #[token("return")]
    Return,

    #[token("assert")]
    Assert,

    #[token("match")]
    Match,

    #[token("import")]
    Import,

    #[token("export")]
    Export,

    #[token("from")]
    From,

    #[token("true")]
    True,

    #[token("false")]
    False,

    // ========================================================================
    // 2. NATURAL BOOLEAN OPERATOR KEYWORDS
    // ========================================================================
    #[token("and")]
    And,

    #[token("or")]
    Or,

    #[token("not")]
    Not,

    // ========================================================================
    // 3. PHYSICAL & HARDWARE DOMAIN KEYWORDS
    // ========================================================================
    #[token("space")]
    Space,

    #[token("module")]
    Module,

    #[token("device")]
    Device,

    #[token("material")]
    Material,

    #[token("profile")]
    Profile,

    #[token("route")]
    Route,

    #[token("test")]
    Test,

    #[token("nets")]
    Nets,

    #[token("pins")]
    Pins,

    #[token("implements")]
    Implements,

    #[token("to")]
    To,

    #[token("with")]
    With,

    #[token("intent")]
    Intent,

    // ========================================================================
    // 4. DELIMITERS & PUNCTUATION
    // ========================================================================
    #[token("{")]
    OpenBrace,

    #[token("}")]
    CloseBrace,

    #[token("(")]
    OpenParen,

    #[token(")")]
    CloseParen,

    #[token("[")]
    OpenBracket,

    #[token("]")]
    CloseBracket,

    #[token(":")]
    Colon,

    #[token(";")]
    Semicolon,

    #[token(",")]
    Comma,

    #[token(".")]
    Dot,

    #[token("->")]
    Arrow,

    #[token("=>")]
    FatArrow,

    // ========================================================================
    // 5. OPERATORS
    // ========================================================================
    #[token("=")]
    Equals,

    #[token("+=")]
    PlusEquals,

    #[token("-=")]
    MinusEquals,

    #[token("*=")]
    StarEquals,

    #[token("/=")]
    SlashEquals,

    #[token("+")]
    Plus,

    #[token("-")]
    Hyphen,

    #[token("*")]
    Asterisk,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[token("==")]
    DoubleEquals,

    #[token("!=")]
    NotEquals,

    #[token("<")]
    LessThan,

    #[token(">")]
    GreaterThan,

    #[token("<=")]
    LessThanOrEqual,

    #[token(">=")]
    GreaterThanOrEqual,

    #[token("..")]
    Range,

    #[token("..=")]
    RangeInclusive,

    #[token("@")]
    AtSymbol,

    // ========================================================================
    // 6. LITERALS & IDENTIFIERS
    // ========================================================================
    /// Import path: @std/layout/sky130
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*(/[a-zA-Z_][a-zA-Z0-9_]*)*", |lex| Some(lex.slice().to_string()))]
    ImportPath(String),

    /// Interpolated identifier: L1_R{row}_C{col}
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*(\{[^}]+\}([a-zA-Z0-9_]+)?)+", priority = 10, callback = parse_interpolated_identifier)]
    InterpolatedIdentifier(Vec<InterpolatedPart>),

    /// Standard Identifier
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| Some(lex.slice().to_string()))]
    Identifier(String),

    /// Float numbers (with optional scientific notation)
    #[regex(r"\d+\.\d+([eE][+-]?\d+)?", priority = 18, callback = |lex| lex.slice().parse::<f64>().ok())]
    #[regex(r"\d+[eE][+-]?\d+", priority = 18, callback = |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),

    /// Integer literals: dec, hex, bin, oct
    #[regex(r"0[xX][0-9a-fA-F]+", parse_any_integer, priority = 17)]
    #[regex(r"0[bB][01]+", parse_any_integer, priority = 17)]
    #[regex(r"0[oO][0-7]+", parse_any_integer, priority = 17)]
    #[regex(r"[0-9]+", parse_any_integer)]
    Integer(i64),

    /// String literals (supporting escape characters)
    #[regex(r#""(?:[^"\\]|\\.)*""#, priority = 20, callback = |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    String(String),

    /// Physical measurement literals (10um, 1.8V, 20mA, 150nm, etc.)
    #[regex(r"\d+\.?\d*([eE][+-]?\d+)?[\p{L}\p{S}\p{M}_µΩ°%·/²³][\p{L}\p{S}\p{M}0-9_µΩ°%·/²³/]*(?:\([\p{L}\p{S}\p{M}0-9µΩ°%·/²³/]+\))*", priority = 16, callback = parse_generic_measurement)]
    Measurement(Measurement),

    // ========================================================================
    // 7. COMMENTS (Skipped / Doc)
    // ========================================================================
    #[regex(r"##\[", priority = 3, callback = parse_doc_block)]
    DocBlock(String),

    #[regex(r"#\[", priority = 2, callback = parse_block_comment)]
    BlockComment(String),

    #[regex(r"##(?:[^\[\n][^\n]*)?", priority = 1, callback = |lex| Some(lex.slice()[2..].trim().to_string()))]
    DocComment(String),

    /// End of file token
    Eof,
}
