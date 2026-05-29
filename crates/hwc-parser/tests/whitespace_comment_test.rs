//! Test that comments and blank lines are properly handled in module bodies

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};

#[test]
fn test_module_with_comments_and_blank_lines() {
    let source = r#"module TestModule:
    pins:
        VCC
        # This is a comment about GND
        GND
        
        # Another comment after a blank line
        DataBus[8]
    
    # Comment before add statement
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
    
    # Comment before route
    route VCC to R1.In
    
    # Comment in for loop
    for i in 0..7:
        # Comment inside loop
        add Bit named B[i]
        
        # Another comment
        route DataBus[i] to B[i].In
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with comments and blank lines"
    );

    assert_eq!(program.definitions.len(), 1);
}

#[test]
fn test_module_with_block_comments() {
    let source = r#"module TestModule:
    pins:
        VCC
        #[ This is a
           multi-line block comment ]#
        GND
    
    #[ Another block comment
       before the add statement ]#
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with block comments"
    );

    assert_eq!(program.definitions.len(), 1);
}

#[test]
fn test_for_loop_with_comments() {
    let source = r#"module TestModule:
    pins:
        Bus[8]
    
    for i in 0..7:
        # This comment should not break the parser
        add Bit named B[i]
        
        # Neither should this one
        route Bus[i] to B[i].In
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with comments in for loop"
    );

    assert_eq!(program.definitions.len(), 1);
}

#[test]
fn test_if_statement_with_comments() {
    let source = r#"module TestModule:
    pins:
        CarryIn
    
    for i in 0..7:
        # Comment before if
        if i = 0:
            # Comment in then branch
            route CarryIn to Bit[i].CarryIn
        else:
            # Comment in else branch
            route Bit[i-1].CarryOut to Bit[i].CarryIn
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with comments in if/else"
    );

    assert_eq!(program.definitions.len(), 1);
}

#[test]
fn test_empty_lines_between_statements() {
    let source = r#"module TestModule:
    pins:
        VCC
        
        
        GND
    
    
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
    
    
    
    route VCC to R1.In
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Parser should succeed with multiple blank lines"
    );

    assert_eq!(program.definitions.len(), 1);
}
