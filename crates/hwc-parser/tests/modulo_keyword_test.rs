//! Test for modulo keyword implementation (Task 3.1)
//!
//! Reference: ROADMAP/v0.1.6/AUTHORITY-IMPLEMENTATION-PLAN.md
//! Task 3.1: Implement Modulo Keyword

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, LogicExpression, LogicOperator, Parser};

#[test]
fn test_mod_keyword_lexes_correctly() {
    let source = "mod";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");

    // Should have: Mod, Eof (no newline for single token)
    assert_eq!(tokens.len(), 2);
    assert!(matches!(tokens[0].token, hwc_parser::Token::Mod));
}

#[test]
fn test_mod_keyword_in_logic_expression() {
    let source = r#"logic CounterLogic:
    let result = count mod 10
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Logic(logic_def) = &program.definitions[0] {
        assert_eq!(logic_def.name.as_str(), "CounterLogic");
        assert_eq!(logic_def.logic_block.statements.len(), 1);

        // Check the let statement
        if let hwc_parser::LogicStatement::Let { expression, .. } =
            &logic_def.logic_block.statements[0]
        {
            // Should be a binary expression with Modulo operator
            if let LogicExpression::Binary {
                operator,
                left,
                right,
                ..
            } = expression
            {
                assert_eq!(*operator, LogicOperator::Modulo);

                // Left should be 'count' variable
                if let LogicExpression::Variable { name, .. } = left.as_ref() {
                    assert_eq!(name, "count");
                } else {
                    panic!("Expected Variable for left operand");
                }

                // Right should be literal 10
                if let LogicExpression::Literal { value, .. } = right.as_ref() {
                    assert_eq!(*value, 10);
                } else {
                    panic!("Expected Literal for right operand");
                }
            } else {
                panic!("Expected Binary expression with Modulo operator");
            }
        } else {
            panic!("Expected Let statement");
        }
    } else {
        panic!("Expected Logic definition");
    }
}

#[test]
fn test_mod_keyword_precedence() {
    let source = r#"logic PrecedenceTest:
    let result = a + b mod c
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    if let hwc_parser::Definition::Logic(logic_def) = &program.definitions[0] {
        if let hwc_parser::LogicStatement::Let { expression, .. } =
            &logic_def.logic_block.statements[0]
        {
            // Should parse as: a + (b mod c) because mod has higher precedence than +
            if let LogicExpression::Binary {
                operator, right, ..
            } = expression
            {
                assert_eq!(*operator, LogicOperator::Add);

                // Right should be (b mod c)
                if let LogicExpression::Binary { operator, .. } = right.as_ref() {
                    assert_eq!(*operator, LogicOperator::Modulo);
                } else {
                    panic!("Expected Binary expression with Modulo on right side");
                }
            } else {
                panic!("Expected Binary expression with Add operator");
            }
        }
    }
}

#[test]
fn test_mod_keyword_with_parentheses() {
    let source = r#"logic ParenTest:
    let result = (count + offset) mod size
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    if let hwc_parser::Definition::Logic(logic_def) = &program.definitions[0] {
        if let hwc_parser::LogicStatement::Let { expression, .. } =
            &logic_def.logic_block.statements[0]
        {
            // Should parse as: (count + offset) mod size
            if let LogicExpression::Binary { operator, left, .. } = expression {
                assert_eq!(*operator, LogicOperator::Modulo);

                // Left should be grouped expression
                if let LogicExpression::Grouped { expression, .. } = left.as_ref() {
                    if let LogicExpression::Binary { operator, .. } = expression.as_ref() {
                        assert_eq!(*operator, LogicOperator::Add);
                    } else {
                        panic!("Expected Add inside grouped expression");
                    }
                } else {
                    panic!("Expected Grouped expression on left side");
                }
            } else {
                panic!("Expected Binary expression with Modulo operator");
            }
        }
    }
}

#[test]
fn test_percent_still_works_as_unit() {
    // Verify that % still works as a unit suffix (not affected by mod keyword)
    let source = r#"component Resistor:
    electrical:
        tolerance: 5%
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    // Should parse successfully - 5% is a measurement token
    assert_eq!(program.definitions.len(), 1);
}

#[test]
fn test_mod_operator_symbol() {
    // Verify that the operator symbol is "mod"
    assert_eq!(LogicOperator::Modulo.symbol(), "mod");
}

#[test]
fn test_mod_operator_precedence_value() {
    // Verify that modulo has same precedence as multiply and divide
    assert_eq!(
        LogicOperator::Modulo.precedence(),
        LogicOperator::Multiply.precedence()
    );
    assert_eq!(
        LogicOperator::Modulo.precedence(),
        LogicOperator::Divide.precedence()
    );
    assert!(LogicOperator::Modulo.precedence() > LogicOperator::Add.precedence());
}
