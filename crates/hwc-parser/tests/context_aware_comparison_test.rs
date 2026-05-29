// Task B6: Context-Aware Comparison Parsing Tests
// Tests that single `=` works for both assignment and comparison based on context

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::ast::{BlockOrExpr, LogicBlock, LogicExpression, LogicOperator, LogicStatement};
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;

fn parse_logic(source: &str) -> Result<LogicBlock, String> {
    let lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error: {:?}", e))?;
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 20);

    match parser.parse_logic_block(&collector) {
        Some(block) => {
            if collector.has_errors() {
                Err("Parse errors occurred".into())
            } else {
                Ok(block)
            }
        }
        None => Err("Failed to parse logic block".into()),
    }
}

#[test]
fn test_if_statement_with_single_equals_comparison() {
    let source = r#"logic:
    if count = 0:
        result = 1
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::If { condition, .. } => {
            // The condition should be a comparison using Equal operator
            match condition {
                LogicExpression::Binary { operator, .. } => {
                    assert_eq!(*operator, LogicOperator::Equal);
                }
                _ => panic!("Expected binary expression for condition"),
            }
        }
        _ => panic!("Expected if statement"),
    }
}

#[test]
fn test_standalone_assignment_with_single_equals() {
    let source = r#"logic:
    count = 5
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::Assignment { .. } => {
            // This is correct - standalone = is assignment
        }
        _ => panic!("Expected assignment statement"),
    }
}

#[test]
fn test_comparison_in_parenthesized_expression() {
    let source = r#"logic:
    let is_ready = (status = 1)
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::Let { expression, .. } => {
            // Inside parentheses, = should be comparison
            match expression {
                LogicExpression::Grouped { expression, .. } => match expression.as_ref() {
                    LogicExpression::Binary { operator, .. } => {
                        assert_eq!(*operator, LogicOperator::Equal);
                    }
                    _ => panic!("Expected binary expression inside parentheses"),
                },
                _ => panic!("Expected grouped expression"),
            }
        }
        _ => panic!("Expected let statement"),
    }
}

#[test]
fn test_match_arm_with_single_equals_comparison() {
    let source = r#"logic:
    let result = match state:
        State.Idle: (count = 0)
        State.Active: (count = 1)
        else: 0
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::Let { expression, .. } => {
            match expression {
                LogicExpression::Match { arms, .. } => {
                    // Check first arm has comparison
                    match &arms[0].body {
                        BlockOrExpr::Expression(expr) => match expr {
                            LogicExpression::Grouped { expression, .. } => {
                                match expression.as_ref() {
                                    LogicExpression::Binary { operator, .. } => {
                                        assert_eq!(*operator, LogicOperator::Equal);
                                    }
                                    _ => panic!("Expected binary expression in match arm"),
                                }
                            }
                            _ => panic!("Expected grouped expression in match arm"),
                        },
                        _ => panic!("Expected expression in match arm body"),
                    }
                }
                _ => panic!("Expected match expression"),
            }
        }
        _ => panic!("Expected let statement"),
    }
}

#[test]
fn test_nested_comparisons() {
    let source = r#"logic:
    if (a = 1) and (b = 2):
        result = 3
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::If { condition, .. } => {
            // The condition should be an AND of two comparisons
            match condition {
                LogicExpression::Binary {
                    operator,
                    left,
                    right,
                    ..
                } => {
                    assert_eq!(*operator, LogicOperator::BitwiseAnd);

                    // Check left comparison
                    match left.as_ref() {
                        LogicExpression::Grouped { expression, .. } => match expression.as_ref() {
                            LogicExpression::Binary { operator, .. } => {
                                assert_eq!(*operator, LogicOperator::Equal);
                            }
                            _ => panic!("Expected binary expression in left group"),
                        },
                        _ => panic!("Expected grouped expression on left"),
                    }

                    // Check right comparison
                    match right.as_ref() {
                        LogicExpression::Grouped { expression, .. } => match expression.as_ref() {
                            LogicExpression::Binary { operator, .. } => {
                                assert_eq!(*operator, LogicOperator::Equal);
                            }
                            _ => panic!("Expected binary expression in right group"),
                        },
                        _ => panic!("Expected grouped expression on right"),
                    }
                }
                _ => panic!("Expected binary expression for condition"),
            }
        }
        _ => panic!("Expected if statement"),
    }
}

#[test]
fn test_comparison_in_inline_if_expression() {
    let source = r#"logic:
    let result = if count = 0: 1 else: 0
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::Let { expression, .. } => {
            match expression {
                LogicExpression::If { condition, .. } => {
                    // The condition should be a comparison
                    match condition.as_ref() {
                        LogicExpression::Binary { operator, .. } => {
                            assert_eq!(*operator, LogicOperator::Equal);
                        }
                        _ => panic!("Expected binary expression for inline if condition"),
                    }
                }
                _ => panic!("Expected if expression"),
            }
        }
        _ => panic!("Expected let statement"),
    }
}

#[test]
fn test_multiple_comparisons_in_sequence() {
    let source = r#"logic:
    if a = 1:
        x = 10
    if b = 2:
        y = 20
    if c = 3:
        z = 30
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 3);

    // All three should be if statements with comparisons
    for statement in &block.statements {
        match statement {
            LogicStatement::If { condition, .. } => match condition {
                LogicExpression::Binary { operator, .. } => {
                    assert_eq!(*operator, LogicOperator::Equal);
                }
                _ => panic!("Expected binary expression for condition"),
            },
            _ => panic!("Expected if statement"),
        }
    }
}

#[test]
fn test_comparison_with_field_access() {
    let source = r#"logic:
    if state.value = 0:
        result = 1
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::If { condition, .. } => {
            match condition {
                LogicExpression::Binary { operator, left, .. } => {
                    assert_eq!(*operator, LogicOperator::Equal);
                    // Left side should be field access
                    match left.as_ref() {
                        LogicExpression::FieldAccess { .. } => {
                            // Correct
                        }
                        _ => panic!("Expected field access on left side"),
                    }
                }
                _ => panic!("Expected binary expression for condition"),
            }
        }
        _ => panic!("Expected if statement"),
    }
}

#[test]
fn test_comparison_with_array_access() {
    let source = r#"logic:
    if data[0] = 0xFF:
        result = 1
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::If { condition, .. } => {
            match condition {
                LogicExpression::Binary { operator, left, .. } => {
                    assert_eq!(*operator, LogicOperator::Equal);
                    // Left side should be array access
                    match left.as_ref() {
                        LogicExpression::ArrayAccess { .. } => {
                            // Correct
                        }
                        _ => panic!("Expected array access on left side"),
                    }
                }
                _ => panic!("Expected binary expression for condition"),
            }
        }
        _ => panic!("Expected if statement"),
    }
}

#[test]
fn test_complex_comparison_expression() {
    let source = r#"logic:
    if (a + b) = (c + d):
        result = 1
"#;

    let block = parse_logic(source).unwrap();
    assert_eq!(block.statements.len(), 1);

    match &block.statements[0] {
        LogicStatement::If { condition, .. } => {
            match condition {
                LogicExpression::Binary {
                    operator,
                    left,
                    right,
                    ..
                } => {
                    assert_eq!(*operator, LogicOperator::Equal);

                    // Both sides should be grouped arithmetic expressions
                    match left.as_ref() {
                        LogicExpression::Grouped { expression, .. } => match expression.as_ref() {
                            LogicExpression::Binary {
                                operator: inner_op, ..
                            } => {
                                assert_eq!(*inner_op, LogicOperator::Add);
                            }
                            _ => panic!("Expected addition in left group"),
                        },
                        _ => panic!("Expected grouped expression on left"),
                    }

                    match right.as_ref() {
                        LogicExpression::Grouped { expression, .. } => match expression.as_ref() {
                            LogicExpression::Binary {
                                operator: inner_op, ..
                            } => {
                                assert_eq!(*inner_op, LogicOperator::Add);
                            }
                            _ => panic!("Expected addition in right group"),
                        },
                        _ => panic!("Expected grouped expression on right"),
                    }
                }
                _ => panic!("Expected binary expression for condition"),
            }
        }
        _ => panic!("Expected if statement"),
    }
}
