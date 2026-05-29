//! Test unit definition parsing (standard library)

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Definition, Lexer, Parser};

#[test]
fn test_parse_simple_unit() {
    let source = r#"unit Microfarad:
    symbol: "µF"
    dimension: capacitance
    description: "Most common capacitor unit"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Unit(unit) = &program.definitions[0] {
        assert_eq!(unit.name.as_str(), "Microfarad");
        assert_eq!(unit.symbol, "µF");
        assert_eq!(unit.dimension, "capacitance");
        assert_eq!(unit.description, Some("Most common capacitor unit".into()));
    } else {
        panic!("Expected Unit definition");
    }
}

#[test]
fn test_parse_unit_with_multiplier() {
    let source = r#"unit Nanofarad:
    symbol: "nF"
    base_si: "F"
    multiplier: 1e-9
    dimension: capacitance
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Unit(unit) = &program.definitions[0] {
        assert_eq!(unit.name.as_str(), "Nanofarad");
        assert_eq!(unit.symbol, "nF");
        assert_eq!(unit.base_si, Some("F".into()));
        assert_eq!(unit.multiplier, Some(1e-9));
        assert_eq!(unit.dimension, "capacitance");
    } else {
        panic!("Expected Unit definition");
    }
}

#[test]
fn test_parse_unit_with_aliases() {
    let source = r#"unit Microfarad:
    symbol: "µF"
    aliases: ["uF", "microF"]
    base_si: "F"
    multiplier: 1e-6
    dimension: capacitance
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Unit(unit) = &program.definitions[0] {
        assert_eq!(unit.name.as_str(), "Microfarad");
        assert_eq!(unit.symbol, "µF");
        assert_eq!(unit.aliases, vec!["uF", "microF"]);
        assert!(unit.matches("µF"));
        assert!(unit.matches("uF"));
        assert!(unit.matches("microF"));
        assert!(!unit.matches("nF"));
    } else {
        panic!("Expected Unit definition");
    }
}

#[test]
fn test_parse_unit_with_examples() {
    let source = r#"unit Percent:
    symbol: "%"
    dimension: ratio
    multiplier: 0.01
    description: "Percentage"
    examples: ["1%", "5%", "10%"]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Unit(unit) = &program.definitions[0] {
        assert_eq!(unit.name.as_str(), "Percent");
        assert_eq!(unit.symbol, "%");
        assert_eq!(unit.dimension, "ratio");
        assert_eq!(unit.examples, vec!["1%", "5%", "10%"]);
    } else {
        panic!("Expected Unit definition");
    }
}
