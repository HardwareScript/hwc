//! Display implementation for Token types
//!
//! Provides human-friendly error messages by converting compiler tokens
//! into conversational descriptions.

use std::fmt;

use super::interpolation::InterpolatedPart;
use super::token_types::Token;

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Translate compiler-speak to human-speak
            Token::InterpolatedIdentifier(parts) => {
                write!(f, "interpolated name '")?;
                for part in parts {
                    match part {
                        InterpolatedPart::Literal(lit) => write!(f, "{}", lit)?,
                        InterpolatedPart::Expression(expr) => write!(f, "{{{}}}", expr)?,
                    }
                }
                write!(f, "'")
            }
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
            Token::Export => write!(f, "the 'export' keyword"),
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
            Token::Region => write!(f, "the 'region' keyword"),
            Token::Inside => write!(f, "the 'inside' keyword"),
            Token::Dimensions => write!(f, "the 'dimensions' keyword"),
            Token::Grid => write!(f, "the 'grid' keyword"),
            Token::Path => write!(f, "the 'path' keyword"),
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
            Token::SpiceModel => write!(f, "the 'spice_model' keyword"),
            Token::Subcircuit => write!(f, "the 'subcircuit' keyword"),
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
