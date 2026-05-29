//! Test for greedy unit consumption (Task 3.2)
//!
//! Reference: ROADMAP/v0.1.6/AUTHORITY-IMPLEMENTATION-PLAN.md
//! Task 3.2: Implement Greedy Unit Consumption
//!
//! Verifies that the lexer consumes NUMBER + UNIT as a single Measurement token,
//! preventing % from ever appearing as a separate token.

use hwc_parser::{Lexer, Token};

#[test]
fn test_percent_lexes_as_single_measurement() {
    let source = "5%";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Should have: Measurement(5, %), Eof
    assert_eq!(tokens.len(), 2);

    // First token should be a Measurement, not separate Number and Percent
    match &tokens[0].token {
        Token::Measurement(m) => {
            assert_eq!(m.value, 5.0);
            assert_eq!(m.unit, hwc_parser::lexer::Unit::Custom("%".to_string()));
        }
        _ => panic!("Expected Measurement token, got {:?}", tokens[0].token),
    }

    // Second token should be Eof
    assert!(matches!(tokens[1].token, Token::Eof));
}

#[test]
fn test_microfarad_lexes_as_single_measurement() {
    let source = "10µF";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Should have: Measurement(10, µF), Eof
    assert_eq!(tokens.len(), 2);

    match &tokens[0].token {
        Token::Measurement(m) => {
            assert_eq!(m.value, 10.0);
            assert_eq!(m.unit, hwc_parser::lexer::Unit::Custom("µF".to_string()));
        }
        _ => panic!("Expected Measurement token, got {:?}", tokens[0].token),
    }
}

#[test]
fn test_percent_never_appears_as_separate_token() {
    // Test various contexts where % should always be part of a measurement
    let test_cases = vec![
        ("1%", 1),   // Simple percentage
        ("5%", 1),   // Another percentage
        ("100%", 1), // Full percentage
        ("0.5%", 1), // Decimal percentage
    ];

    for (source, expected_non_eof_tokens) in test_cases {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        // Count non-EOF tokens
        let non_eof_count = tokens
            .iter()
            .filter(|t| !matches!(t.token, Token::Eof))
            .count();
        assert_eq!(
            non_eof_count, expected_non_eof_tokens,
            "Failed for: {}",
            source
        );

        // Verify no Token::Percent exists
        for token in &tokens {
            assert!(
                !matches!(token.token, Token::Percent),
                "Found standalone Percent token in: {}",
                source
            );
        }
    }
}

#[test]
fn test_number_without_unit_is_separate() {
    let source = "42";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Should have: Integer(42), Eof
    assert_eq!(tokens.len(), 2);
    assert!(matches!(tokens[0].token, Token::Integer(42)));
}

#[test]
fn test_greedy_consumption_with_scientific_notation() {
    let source = "1e-3%";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Should lex as single Measurement token
    assert_eq!(tokens.len(), 2);

    match &tokens[0].token {
        Token::Measurement(m) => {
            assert_eq!(m.value, 0.001);
            assert_eq!(m.unit, hwc_parser::lexer::Unit::Custom("%".to_string()));
        }
        _ => panic!("Expected Measurement token for scientific notation with unit"),
    }
}

#[test]
fn test_greedy_consumption_prevents_ambiguity() {
    // This is the key test: verify that "5%" cannot be parsed as "5" followed by "%"
    let source = "tolerance: 5%";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Should have: Identifier("tolerance"), Colon, Measurement(5, %), Eof
    assert_eq!(tokens.len(), 4);

    assert!(matches!(tokens[0].token, Token::Identifier(_)));
    assert!(matches!(tokens[1].token, Token::Colon));

    // The critical assertion: token[2] must be Measurement, not Integer
    match &tokens[2].token {
        Token::Measurement(m) => {
            assert_eq!(m.value, 5.0);
            assert_eq!(m.unit, hwc_parser::lexer::Unit::Custom("%".to_string()));
        }
        Token::Integer(_) => {
            panic!("Greedy consumption failed! Got Integer instead of Measurement")
        }
        _ => panic!("Expected Measurement token"),
    }
}

#[test]
fn test_parser_never_sees_percent_as_operator() {
    // Verify that in a property context, % is always part of a measurement
    let source = r#"component Resistor:
    electrical:
        tolerance: 5%
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Scan all tokens - should never find Token::Percent
    for token in &tokens {
        assert!(
            !matches!(token.token, Token::Percent),
            "Found standalone Percent token in component definition"
        );
    }

    // Should find exactly one Measurement token with % unit
    let measurement_count = tokens
        .iter()
        .filter(|t| matches!(&t.token, Token::Measurement(m) if m.unit == hwc_parser::lexer::Unit::Custom("%".to_string())))
        .count();

    assert_eq!(
        measurement_count, 1,
        "Should find exactly one 5% measurement"
    );
}

#[test]
fn test_no_space_required_for_greedy_consumption() {
    // Verify that the lexer requires NO SPACE between number and unit
    let with_space = "5 %";
    let without_space = "5%";

    let lexer1 = Lexer::new(with_space);
    let tokens1 = lexer1.tokenize().unwrap();

    let lexer2 = Lexer::new(without_space);
    let tokens2 = lexer2.tokenize().unwrap();

    // With space: should be Integer(5), Percent, Eof (3 tokens)
    // Without space: should be Measurement(5, %), Eof (2 tokens)
    assert_eq!(tokens1.len(), 3, "With space should produce 3 tokens");
    assert_eq!(tokens2.len(), 2, "Without space should produce 2 tokens");

    // Verify the without-space case is a Measurement
    assert!(matches!(tokens2[0].token, Token::Measurement(_)));
}
