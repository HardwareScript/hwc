//! Number parsing helpers for integer literals with different bases
//!
//! Supports decimal, hexadecimal (0x), binary (0b), and octal (0o) integers.
//!
//! **CRITICAL FIX (Sprint 3.9 - "Lexer Greed" Bug)**:
//! Signs are now separate operator tokens, not part of number literals.
//! The parser handles unary operators (e.g., `-10`) by combining tokens.

use super::Token;

/// Parse integer literals with support for different bases
///
/// Supports:
/// - Decimal: 42, -100
/// - Hexadecimal: 0xFF, 0x1A2B
/// - Binary: 0b1010, 0b11110000
/// - Octal: 0o77, 0o755
///
/// **Important**: For decimal integers, Rust's native parser handles the sign correctly,
/// including i64::MIN (-9223372036854775808). For other bases, we manually handle signs
/// because from_str_radix doesn't accept them.
pub fn parse_any_integer(lex: &mut logos::Lexer<Token>) -> Option<i64> {
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
