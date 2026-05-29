// Test D1: Context-Aware Error Messages
// Tests that v0.1.6 provides helpful, educational error messages

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;

#[test]
fn test_define_keyword_error() {
    let source = r#"
define component Resistor:
    pins: [A, B]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);
    assert!(
        collector.has_errors(),
        "Should have errors for 'define' keyword"
    );
}

#[test]
fn test_quoted_identifier_error() {
    let source = r#"
component "Resistor":
    pins: [A, B]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);
    assert!(
        collector.has_errors(),
        "Should have errors for quoted identifier"
    );
}

#[test]
fn test_equals_in_property_block_error() {
    let source = r#"
component Resistor:
    pins: [A, B]
    electrical:
        resistance = 10kΩ
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);
    assert!(
        collector.has_errors(),
        "Should have errors for '=' in property block"
    );
}

#[test]
fn test_correct_v016_syntax_works() {
    let source = r#"
component Resistor:
    pins: [A, B]
    electrical:
        resistance: 10kΩ
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);

    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Should parse correctly with v0.1.6 syntax"
    );
}
