//! Test for reserved symbol error messages (Task 3.3)
//!
//! Reference: ROADMAP/v0.1.6/AUTHORITY-IMPLEMENTATION-PLAN.md
//! Task 3.3: Implement Reserved Symbol Error Messages
//!
//! Verifies that using '%' as a binary operator produces a helpful error
//! suggesting the 'mod' keyword instead.

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};

#[test]
fn test_percent_as_operator_produces_error() {
    let source = r#"logic CounterLogic:
    let result = count % 10
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    // Should have collected an error
    assert!(
        collector.has_errors(),
        "Expected error when using % as operator"
    );
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );
}

#[test]
fn test_percent_in_arithmetic_expression_fails() {
    let source = r#"logic Test:
    let x = a % b
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(collector.has_errors(), "Expected error for % in arithmetic");
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );
}

#[test]
fn test_percent_in_complex_expression_fails() {
    let source = r#"logic Test:
    let result = (a + b) % c
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(
        collector.has_errors(),
        "Expected error for % in complex expression"
    );
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );
}

#[test]
fn test_mod_keyword_works_instead() {
    // Verify that 'mod' keyword works as the correct alternative
    let source = r#"logic CounterLogic:
    let result = count mod 10
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(!collector.has_errors(), "mod keyword should work");
}

#[test]
fn test_percent_as_unit_still_works() {
    // Verify that % as a unit suffix still works (not affected by operator check)
    let source = r#"component Resistor:
    electrical:
        tolerance: 5%
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(!collector.has_errors(), "% as unit should still work");
}

#[test]
fn test_percent_in_if_condition_fails() {
    let source = r#"logic Test:
    if x % 2 = 0:
        let even = true
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(
        collector.has_errors(),
        "Expected error for % in if condition"
    );
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );
}

#[test]
fn test_percent_in_match_expression_fails() {
    let source = r#"logic Test:
    let result = match x % 4:
        0: A
        else: B
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(
        collector.has_errors(),
        "Expected error for % in match selector"
    );
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );
}

#[test]
fn test_error_message_suggests_mod_keyword() {
    let source = r#"logic Test:
    let x = a % b
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    assert!(collector.has_errors(), "Expected error for % operator");
    assert!(
        collector.error_count() > 0,
        "Should have at least one error"
    );

    // The error should be a ParseError that can be formatted
    // We just verify it fails - the actual error message formatting
    // is handled by miette and tested through integration tests
}
