//! Lexer tests for string literal handling
//!
//! These tests verify that the lexer correctly handles all characters inside string literals,
//! including punctuation that could be mistaken for tokens (comma, dot, angle brackets, etc.)

use hwc_parser::lexer::{Lexer, Token};

/// Helper function to tokenize input and extract tokens (ignoring whitespace/EOF)
fn tokenize(input: &str) -> Vec<Token> {
    let lexer = Lexer::new(input);
    let spanned_tokens = lexer.tokenize().expect("Lexer should not fail");

    // Extract just the tokens, filter out Newline/Indent/Dedent/Eof
    spanned_tokens
        .into_iter()
        .map(|st| st.token)
        .filter(|t| {
            !matches!(
                t,
                Token::Newline | Token::Indent | Token::Dedent | Token::Eof
            )
        })
        .collect()
}

#[test]
fn test_string_with_comma() {
    let input = r#"description: "Test, with, commas""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "Test, with, commas"));
}

#[test]
fn test_string_with_dot() {
    let input = r#"notes: "This is a sentence. With periods.""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "This is a sentence. With periods."));
}

#[test]
fn test_string_with_question_mark() {
    let input = r#"question: "Does this work?""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "Does this work?"));
}

#[test]
fn test_string_with_single_quote() {
    let input = r#"text: "He said 'hello' to me""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "He said 'hello' to me"));
}

#[test]
fn test_string_with_backtick() {
    let input = r#"code: "Use `inline code` here""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "Use `inline code` here"));
}

#[test]
fn test_string_with_angle_brackets() {
    let input = r#"comparison: "Is 5 < 10 or 10 > 5?""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "Is 5 < 10 or 10 > 5?"));
}

#[test]
fn test_string_with_all_punctuation() {
    let input = r#"special: "Test: '`<>,.?/!@#$%^&*()[]{}|\\;""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "Test: '`<>,.?/!@#$%^&*()[]{}|\\\\;"));
}

#[test]
fn test_string_with_escaped_quote() {
    let input = r#"text: "He said \"hello\" loudly""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    // Note: The lexer removes the outer quotes but keeps the escaped quotes
    assert!(matches!(tokens[2], Token::String(ref s) if s == r#"He said \"hello\" loudly"#));
}

#[test]
fn test_string_with_unicode() {
    let input = r#"description: "Tests unicode: Ω µ ° ± × ÷ ≤ ≥ ≈ ≠ → ← ↑ ↓ ∞ √ ∑ ∫ π""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(
        matches!(tokens[2], Token::String(ref s) if s == "Tests unicode: Ω µ ° ± × ÷ ≤ ≥ ≈ ≠ → ← ↑ ↓ ∞ √ ∑ ∫ π")
    );
}

#[test]
fn test_string_with_emoji() {
    let input = r#"note: "High power 🔥 component""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "High power 🔥 component"));
}

#[test]
fn test_multiple_strings_on_same_line() {
    let input = r#"a: "first, string" b: "second. string?""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 6);
    assert!(matches!(tokens[0], Token::Identifier(ref s) if s == "a"));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "first, string"));
    assert!(matches!(tokens[3], Token::Identifier(ref s) if s == "b"));
    assert_eq!(tokens[4], Token::Colon);
    assert!(matches!(tokens[5], Token::String(ref s) if s == "second. string?"));
}

#[test]
fn test_empty_string() {
    let input = r#"empty: """#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s.is_empty()));
}

#[test]
fn test_string_with_numbers_and_units() {
    let input = r#"spec: "Voltage: 3.3V, Current: 100mA, Resistance: 4.7kΩ""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(
        matches!(tokens[2], Token::String(ref s) if s == "Voltage: 3.3V, Current: 100mA, Resistance: 4.7kΩ")
    );
}

#[test]
fn test_string_priority_over_measurement() {
    // Ensure string literal takes priority over measurement parsing
    let input = r#"text: "10mm is a measurement""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    // Should be parsed as a string, not as separate tokens
    assert!(matches!(tokens[2], Token::String(ref s) if s == "10mm is a measurement"));
}

#[test]
fn test_string_with_colon_inside() {
    let input = r#"time: "12:30:45""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "12:30:45"));
}

#[test]
fn test_string_with_brackets() {
    let input = r#"array: "data[0], data[1], data[2]""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "data[0], data[1], data[2]"));
}

#[test]
fn test_string_with_parentheses() {
    let input = r#"formula: "f(x) = (a + b) * c""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "f(x) = (a + b) * c"));
}

#[test]
fn test_string_with_equals() {
    let input = r#"equation: "x = y + z""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "x = y + z"));
}

#[test]
fn test_string_with_slash() {
    let input = r#"filepath: "C:/Users/Documents/file.txt""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == "C:/Users/Documents/file.txt"));
}

#[test]
fn test_string_with_backslash() {
    let input = r#"filepath: "C:\\Users\\Documents\\file.txt""#;
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0], Token::Identifier(_)));
    assert_eq!(tokens[1], Token::Colon);
    assert!(matches!(tokens[2], Token::String(ref s) if s == r"C:\\Users\\Documents\\file.txt"));
}
