//! Test inline comma-separated pin syntax

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};

#[test]
fn test_component_inline_pins() {
    let source = r#"component I2C_Sensor:
    pins: VCC, GND, SDA, SCL
    
    layout:
        shape: Rectangle(5mm, 5mm, 1mm)
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with inline pins"
    );

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "I2C_Sensor");
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
fn test_component_block_pins_still_works() {
    let source = r#"component I2C_Sensor:
    pins:
        VCC
        GND
        SDA
        SCL
    
    layout:
        shape: Rectangle(5mm, 5mm, 1mm)
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with block pins"
    );

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "I2C_Sensor");
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
fn test_module_inline_pins() {
    let source = r#"module SimpleModule:
    pins: VCC, GND, DataIn, DataOut
    
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with inline module pins"
    );

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Module(module) = &program.definitions[0] {
        assert_eq!(module.name.as_str(), "SimpleModule");
        assert_eq!(module.pins.len(), 4);
        assert_eq!(module.pins[0].name.as_str(), "VCC");
        assert_eq!(module.pins[1].name.as_str(), "GND");
        assert_eq!(module.pins[2].name.as_str(), "DataIn");
        assert_eq!(module.pins[3].name.as_str(), "DataOut");
    } else {
        panic!("Expected module definition");
    }
}

#[test]
fn test_module_inline_pins_with_arrays() {
    let source = r#"module BusModule:
    pins: VCC, GND, DataBus[8], AddressBus[16]
    
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with inline array pins"
    );

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Module(module) = &program.definitions[0] {
        assert_eq!(module.name.as_str(), "BusModule");
        assert_eq!(module.pins.len(), 4);
        assert_eq!(module.pins[0].name.as_str(), "VCC");
        assert_eq!(module.pins[0].array_size, None);
        assert_eq!(module.pins[1].name.as_str(), "GND");
        assert_eq!(module.pins[1].array_size, None);
        assert_eq!(module.pins[2].name.as_str(), "DataBus");
        assert_eq!(module.pins[2].array_size, Some(8));
        assert_eq!(module.pins[3].name.as_str(), "AddressBus");
        assert_eq!(module.pins[3].array_size, Some(16));
    } else {
        panic!("Expected module definition");
    }
}

#[test]
fn test_module_block_pins_still_works() {
    let source = r#"module SimpleModule:
    pins:
        VCC
        GND
        DataIn
        DataOut
    
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with block module pins"
    );

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Module(module) = &program.definitions[0] {
        assert_eq!(module.name.as_str(), "SimpleModule");
        assert_eq!(module.pins.len(), 4);
    } else {
        panic!("Expected module definition");
    }
}

#[test]
fn test_inline_pins_with_trailing_comma_rejected() {
    let source = r#"component Test:
    pins: VCC, GND,
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let _result = parser.parse(&collector);

    // Should fail because trailing comma is followed by newline, not another identifier
    assert!(collector.has_errors());
}
