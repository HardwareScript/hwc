//! Tests for pattern and strategy parsing

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::ast::*;
use hwc_parser::lexer::Lexer;
use hwc_parser::Parser;

#[test]
fn test_parse_zigzag_pattern() {
    let source = r#"
pattern Zigzag (gap: Measurement):
    steps:
        - gap r 45
        - gap r -45
        - gap r -45
        - gap r 45
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Pattern(pattern) = &program.definitions[0] {
        assert_eq!(pattern.name.as_str(), "Zigzag");
        assert_eq!(pattern.params.len(), 1);
        assert_eq!(pattern.params[0].name.as_str(), "gap");
        assert_eq!(pattern.steps.len(), 4);
    } else {
        panic!("Expected Pattern definition");
    }
}

#[test]
fn test_parse_trombone_pattern() {
    let source = r#"
pattern Trombone (gap: Measurement, amp: Measurement):
    steps:
        - gap r 0
        - amp r 90
        - gap * 2 r 0
        - amp r -90
        - gap r 0
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Pattern(pattern) = &program.definitions[0] {
        assert_eq!(pattern.name.as_str(), "Trombone");
        assert_eq!(pattern.params.len(), 2);
        assert_eq!(pattern.params[0].name.as_str(), "gap");
        assert_eq!(pattern.params[1].name.as_str(), "amp");
        assert_eq!(pattern.steps.len(), 5);
    } else {
        panic!("Expected Pattern definition");
    }
}

#[test]
fn test_parse_strategy_with_pattern() {
    let source = r#"
strategy DDR5_Match:
    target: match_longest
    tolerance: 0.1mm
    pattern: Trombone(gap: 0.3mm, amp: 2.5mm)
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Strategy(strategy) = &program.definitions[0] {
        assert_eq!(strategy.name.as_str(), "DDR5_Match");
        assert!(strategy.target.is_some());
        assert!(strategy.tolerance.is_some());
        assert!(strategy.pattern.is_some());

        if let Some(StrategyTarget::MatchLongest) = strategy.target {
            // Correct
        } else {
            panic!("Expected MatchLongest target");
        }

        if let Some(pattern) = &strategy.pattern {
            assert_eq!(pattern.name.as_str(), "Trombone");
            assert_eq!(pattern.arguments.len(), 2);
        }
    } else {
        panic!("Expected Strategy definition");
    }
}

#[test]
fn test_parse_strategy_with_specific_length() {
    let source = r#"
strategy FixedLength:
    target: 50mm
    tolerance: 0.05mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Strategy(strategy) = &program.definitions[0] {
        assert_eq!(strategy.name.as_str(), "FixedLength");

        if let Some(StrategyTarget::Specific(_)) = strategy.target {
            // Correct
        } else {
            panic!("Expected Specific target");
        }
    } else {
        panic!("Expected Strategy definition");
    }
}

#[test]
fn test_parse_complete_routing_example() {
    let source = r#"
pattern Trombone (gap: Measurement, amp: Measurement):
    steps:
        - gap r 0
        - amp r 90
        - gap * 2 r 0
        - amp r -90
        - gap r 0

strategy DDR5_Match:
    target: match_longest
    tolerance: 0.1mm
    pattern: Trombone(gap: 0.3mm, amp: 2.5mm)
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 2);

    // First should be pattern
    if let Definition::Pattern(pattern) = &program.definitions[0] {
        assert_eq!(pattern.name.as_str(), "Trombone");
    } else {
        panic!("Expected Pattern definition");
    }

    // Second should be strategy
    if let Definition::Strategy(strategy) = &program.definitions[1] {
        assert_eq!(strategy.name.as_str(), "DDR5_Match");
    } else {
        panic!("Expected Strategy definition");
    }
}
