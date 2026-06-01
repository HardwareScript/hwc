#[cfg(test)]
mod tests {
    use crate::ast::{Condition, ModuleDefinition, ModuleStatement};
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::DiagnosticCollector;

    fn parse_module(source: &str) -> Result<ModuleDefinition, String> {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(collector.summary().to_string());
        }

        assert_eq!(
            program.definitions.len(),
            1,
            "Expected exactly one definition"
        );

        if let crate::ast::Definition::Module(module) =
            program.definitions.into_iter().next().unwrap()
        {
            Ok(module)
        } else {
            panic!("Expected module definition");
        }
    }

    #[test]
    fn test_parse_simple_module() {
        let source = r#"module LED_Driver:
    pins:
        VCC
        GND
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
    route VCC to R1.In
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.name.as_str(), "LED_Driver");
        assert_eq!(module.pins.len(), 2);
        assert_eq!(module.statements.len(), 2);
    }

    #[test]
    fn test_parse_module_with_array_pins() {
        let source = r#"module ALU_64Bit:
    pins:
        Bus_A[64]
        Bus_B[64]
        CarryIn
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.pins.len(), 3);
        assert_eq!(module.pins[0].array_size, Some(64));
        assert_eq!(module.pins[1].array_size, Some(64));
        assert_eq!(module.pins[2].array_size, None);
    }

    #[test]
    fn test_parse_module_with_for_loop() {
        let source = r#"module ALU:
    pins:
        Bus[8]
    for i in 0..7:
        add Bit named B[i]
        route Bus[i] to B[i].In
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.statements.len(), 1);

        if let ModuleStatement::For(for_loop) = &module.statements[0] {
            assert_eq!(for_loop.variable, "i");
            assert_eq!(for_loop.start, 0);
            assert_eq!(for_loop.end, 7);
            assert_eq!(for_loop.body.len(), 2);
        } else {
            panic!("Expected for loop");
        }
    }

    #[test]
    fn test_parse_module_with_if_conditional() {
        let source = r#"module ALU:
    pins:
        CarryIn
    for i in 0..7:
        if i == 0:
            route CarryIn to Bit[i].CarryIn
        else:
            route Bit[i - 1].CarryOut to Bit[i].CarryIn
"#;

        let module = parse_module(source).unwrap();

        if let ModuleStatement::For(for_loop) = &module.statements[0] {
            if let ModuleStatement::If(if_stmt) = &for_loop.body[0] {
                assert!(matches!(if_stmt.condition, Condition::Equals { .. }));
                assert_eq!(if_stmt.then_body.len(), 1);
                assert!(if_stmt.else_body.is_some());
            } else {
                panic!("Expected if statement");
            }
        } else {
            panic!("Expected for loop");
        }
    }

    #[test]
    fn test_parse_module_rejects_negative_array_index() {
        let source = r#"module Test:
    pins:
        Bus[64]
    route Bus[-1] to GND
"#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        assert!(
            collector.has_errors(),
            "Parser should reject negative array index Bus[-1]"
        );
    }

    #[test]
    fn test_parse_module_rejects_negative_component_index() {
        let source = r#"module Test:
    pins:
        Out
    route Comp[-5].Pin to Out
"#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        assert!(
            collector.has_errors(),
            "Parser should reject negative component index Comp[-5]"
        );
    }

    #[test]
    fn test_parse_module_accepts_arithmetic_subtraction() {
        let source = r#"module Test:
    pins:
        Bus[64]
    for i in 0..64:
        if i > 0:
            route Bus[i-1] to Bus[i]
"#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        assert!(
            !collector.has_errors(),
            "Parser should accept arithmetic expression i-1: {}",
            collector.summary()
        );
    }
}
