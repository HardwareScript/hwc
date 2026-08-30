//! HardwareScript v0.3.0 Canonical Token Type Definitions
//!
//! Clean, unified lexical specification. Zero whitespace hacks.
//! Block scoping is strictly delimited by `{` and `}`.

use logos::Logos;
use super::super::parsers::*;
use super::super::units::Measurement;
use super::number_parsers::parse_any_integer;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = ())]
#[logos(skip r"[ \t\r\n\f]+")]                     // Whitespace is strictly a separator
#[logos(skip r"//[^\n]*")]                          // C-style single-line comments
#[logos(skip r"#(?:[^#\[\n][^\n]*)?")]             // HardwareScript '#' comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]             // Multi-line block comments
pub enum Token {
    // ========================================================================
    // 1. CORE COMPTIME CONTROL FLOW & DECLARATION KEYWORDS
    // ========================================================================
    #[token("impl")]        Impl,
    #[token("fn")]          Fn,
    #[token("let")]         Let,
    #[token("mut")]         Mut,
    #[token("const")]       Const,
    #[token("struct")]      Struct,
    #[token("enum")]        Enum,
    #[token("if")]          If,
    #[token("else")]        Else,
    #[token("for")]         For,
    #[token("in")]          In,
    #[token("break")]       Break,
    #[token("continue")]    Continue,
    #[token("return")]      Return,
    #[token("assert")]      Assert,
    #[token("match")]       Match,
    #[token("import")]      Import,
    #[token("export")]      Export,
    #[token("from")]        From,
    #[token("true")]        True,
    #[token("false")]       False,

    // ========================================================================
    // 2. NATURAL BOOLEAN OPERATOR KEYWORDS
    // ========================================================================
    #[token("and")]         And,
    #[token("or")]          Or,
    #[token("not")]         Not,

    // ========================================================================
    // ========================================================================
    // 3. PHYSICAL SYNTHESIS & BEHAVIORAL LOGIC KEYWORDS
    // ========================================================================
    #[token("space")]       Space,
    #[token("module")]      Module,
    #[token("device")]      Device,
    #[token("material")]    Material,
    #[token("profile")]     Profile,
    #[token("route")]       Route,
    #[token("test")]        Test,
    #[token("implements")]  Implements,
    #[token("to")]          To,
    #[token("logic")]       Logic,
    #[token("reg")]         Reg,
    #[token("on")]          On,
    #[token("reset_to")]    ResetTo,
    #[token("when")]        When,
    #[token("key")]         Key,
    #[token("region")]      Region,
    #[token("synthesize")]  Synthesize,

    // ========================================================================
    // 4. DELIMITERS & PUNCTUATION
    // ========================================================================
    #[token("#[")]          HashBracket,
    #[token("{")]           OpenBrace,
    #[token("}")]           CloseBrace,
    #[token("(")]           OpenParen,
    #[token(")")]           CloseParen,
    #[token("[")]           OpenBracket,
    #[token("]")]           CloseBracket,
    #[token("::")]          DoubleColon,
    #[token(":")]           Colon,
    #[token(";")]           Semicolon,
    #[token(",")]           Comma,
    #[token(".")]           Dot,
    #[token("->")]          Arrow,
    #[token("=>")]          FatArrow,
    #[token("_")]           Underscore,

    // ========================================================================
    // 5. ARITHMETIC, BITWISE & COMPARISON OPERATORS
    // ========================================================================
    // Assignment
    #[token("=")]           Equals,
    #[token("+=")]          PlusEquals,
    #[token("-=")]          MinusEquals,
    #[token("*=")]          StarEquals,
    #[token("/=")]          SlashEquals,
    #[token("%=")]          PercentEquals,

    // Arithmetic
    #[token("+")]           Plus,
    #[token("-")]           Hyphen,
    #[token("*")]           Asterisk,
    #[token("/")]           Slash,
    #[token("%")]           Percent,

    // Bitwise (Essential for Hardware Register/Bus Math)
    #[token("&")]           Ampersand,
    #[token("|")]           Pipe,
    #[token("^")]           Caret,
    #[token("~")]           Tilde,
    #[token("<<")]          ShiftLeft,
    #[token(">>")]          ShiftRight,

    // Comparison
    #[token("==")]          DoubleEquals,
    #[token("!=")]          NotEquals,
    #[token("<")]           LessThan,
    #[token(">")]           GreaterThan,
    #[token("<=")]          LessThanOrEqual,
    #[token(">=")]          GreaterThanOrEqual,

    // Ranges
    #[token("..")]          Range,
    #[token("..=")]         RangeInclusive,
    #[token("@")]           AtSymbol,

    // ========================================================================
    // 6. LITERALS & IDENTIFIERS
    // ========================================================================
    /// Import path: @std/layout/sky130
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*(/[a-zA-Z_][a-zA-Z0-9_]*)*", |lex| Some(lex.slice().to_string()))]
    ImportPath(String),

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
    // 7. DOCUMENTATION COMMENTS
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
