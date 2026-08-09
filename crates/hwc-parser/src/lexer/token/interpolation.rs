//! Interpolated identifier parsing for template-style name generation
//!
//! v0.2.1: Added InterpolatedIdentifier for modern template-style name generation
//! Example: `L1_R{row}_C{col}` compiles to individual names at compile time

use super::Token;

/// Part of an interpolated identifier - either literal text or an expression
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedPart {
    /// Literal text part: "L1_R", "_C", etc.
    Literal(String),
    /// Expression part (unparsed source text): "row", "col", "i+1", etc.
    /// Will be parsed into Expression AST later by the parser
    Expression(String),
}

/// Parse an interpolated identifier like: L1_R{row}_C{col}
/// Returns a vector alternating between Literal and Expression parts
pub fn parse_interpolated_identifier(
    lex: &mut logos::Lexer<Token>,
) -> Option<Vec<InterpolatedPart>> {
    let source = lex.slice();
    let mut parts = Vec::new();
    let mut current_pos = 0;

    while current_pos < source.len() {
        // Find the next {
        if let Some(brace_start) = source[current_pos..].find('{') {
            let abs_brace_start = current_pos + brace_start;

            // Add literal part before the brace (if any)
            if brace_start > 0 {
                parts.push(InterpolatedPart::Literal(
                    source[current_pos..abs_brace_start].to_string(),
                ));
            }

            // Find the matching }
            if let Some(brace_end) = source[abs_brace_start..].find('}') {
                let abs_brace_end = abs_brace_start + brace_end;

                // Extract expression between braces
                let expr = source[abs_brace_start + 1..abs_brace_end].to_string();
                parts.push(InterpolatedPart::Expression(expr));

                current_pos = abs_brace_end + 1;
            } else {
                // Unmatched brace - shouldn't happen with regex, but be safe
                return None;
            }
        } else {
            // No more braces - add remaining literal (if any)
            if current_pos < source.len() {
                parts.push(InterpolatedPart::Literal(source[current_pos..].to_string()));
            }
            break;
        }
    }

    Some(parts)
}
