//! Tests for list parser (canonical bracket syntax only, post-cleanup)
//!
//! Only bracket notation supported:
//!   `[A, B, C]` (trailing comma ok, multiline ok)
//!
//! Legacy inline and block formats REMOVED in pre-release cleanup (see helpers.rs:652 for rationale).
//! These tests for legacy now assert rejection.

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::lexer::Lexer;
use hwc_parser::Parser;

#[test]
fn test_bracket_notation_simple() {
    let source = r#"component Test:
    pins: [VCC, GND, SDA]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 3);
        assert_eq!(comp.pins[0], "VCC");
        assert_eq!(comp.pins[1], "GND");
        assert_eq!(comp.pins[2], "SDA");
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_bracket_notation_with_trailing_comma() {
    let source = r#"component Test:
    pins: [VCC, GND, SDA,]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 3);
        assert_eq!(comp.pins[0], "VCC");
        assert_eq!(comp.pins[1], "GND");
        assert_eq!(comp.pins[2], "SDA");
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_bracket_notation_empty_list() {
    let source = r#"component Test:
    pins: []
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 0);
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_bracket_notation_single_item() {
    let source = r#"component Test:
    pins: [VCC]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 1);
        assert_eq!(comp.pins[0], "VCC");
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_bracket_notation_multiline() {
    let source = r#"component Test:
    pins: [
        VCC,
        GND,
        SDA,
        SCL
    ]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 4);
        assert_eq!(comp.pins[0], "VCC");
        assert_eq!(comp.pins[1], "GND");
        assert_eq!(comp.pins[2], "SDA");
        assert_eq!(comp.pins[3], "SCL");
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_legacy_inline_format_now_rejected() {
    // Legacy bare comma list syntax was removed to keep parser simple (only brackets).
    // See hwc-parser/src/parser/helpers.rs for removal comment and pattern to avoid.
    let source = r#"component Test:
    pins: VCC, GND, SDA
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&DiagnosticCollector::new("", 100));

    // With legacy syntax, list parser now errors and syncs; expect no definitions parsed.
    assert!(ast.definitions.is_empty(), "Legacy inline list syntax must be rejected post-cleanup");
}

#[test]
fn test_legacy_block_format_now_rejected() {
    // Legacy indented block list syntax removed pre-release.
    let source = r#"component Test:
    pins:
        VCC
        GND
        SDA
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&DiagnosticCollector::new("", 100));

    assert!(ast.definitions.is_empty(), "Legacy block list syntax must be rejected post-cleanup");
}

#[test]
fn test_bracket_notation_with_array_syntax() {
    let source = r#"module Test:
    pins: [VCC, GND, DataBus[8], AddrBus[16]]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Module(module) = &ast.definitions[0] {
        assert_eq!(module.pins.len(), 4);
        assert_eq!(module.pins[0].name, "VCC");
        assert_eq!(module.pins[0].array_size, None);
        assert_eq!(module.pins[1].name, "GND");
        assert_eq!(module.pins[1].array_size, None);
        assert_eq!(module.pins[2].name, "DataBus");
        assert_eq!(module.pins[2].array_size, Some(8));
        assert_eq!(module.pins[3].name, "AddrBus");
        assert_eq!(module.pins[3].array_size, Some(16));
    } else {
        panic!("Expected module definition");
    }
}

#[test]
fn test_module_bracket_notation() {
    let source = r#"module SimpleModule:
    pins: [VCC, GND, DataIn, DataOut]
    
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Module(module) = &ast.definitions[0] {
        assert_eq!(module.pins.len(), 4);
        assert_eq!(module.pins[0].name, "VCC");
        assert_eq!(module.pins[1].name, "GND");
        assert_eq!(module.pins[2].name, "DataIn");
        assert_eq!(module.pins[3].name, "DataOut");
    } else {
        panic!("Expected module definition");
    }
}

#[test]
fn test_bracket_notation_with_whitespace() {
    let source = r#"component Test:
    pins: [ VCC , GND , SDA ]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing

    let ast = result;
    assert_eq!(ast.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &ast.definitions[0] {
        assert_eq!(comp.pins.len(), 3);
        assert_eq!(comp.pins[0], "VCC");
        assert_eq!(comp.pins[1], "GND");
        assert_eq!(comp.pins[2], "SDA");
    } else {
        panic!("Expected component definition");
    }
}

// Note: Enum parsing uses a more complex structure (variants with optional values)
// and doesn't need the simple universal list parser. The current enum parser
// handles: enum Opcode: Add = 0x1, Sub = 0x2, Mul = 0x3
// This is more sophisticated than a simple identifier list.
