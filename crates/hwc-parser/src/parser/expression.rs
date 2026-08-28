//! HardwareScript v0.3.0 Pratt Expression Parser

use crate::ast::{
    BinaryOperator, ElseBranchExpr, Expression, FieldInit, MatchArmBody, MatchArmExpr,
    NamedOrPositionalArg, Pattern, Span, UnaryOperator,
};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse an expression using Pratt parsing with operator precedence hierarchy
    pub fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_expression_with_precedence(0)
    }

    /// Parse expression with minimum precedence (Pratt precedence climbing)
    pub fn parse_expression_with_precedence(
        &mut self,
        min_precedence: u8,
    ) -> Result<Expression, ParseError> {
        let mut left = self.parse_prefix_expression()?;

        // Pratt loop: handle range formation and binary operators
        loop {
            // Check for range operators `..` and `..=` (Precedence Level 4)
            if self.check(&Token::Range) || self.check(&Token::RangeInclusive) {
                const RANGE_PRECEDENCE: u8 = 4;
                if RANGE_PRECEDENCE < min_precedence {
                    break;
                }

                let is_inclusive = self.check(&Token::RangeInclusive);
                self.advance(); // consume `..` or `..=`

                // Right operand of range
                let right = self.parse_expression_with_precedence(RANGE_PRECEDENCE + 1)?;
                let span = Span::new(left.span().start, right.span().end);
                left = Expression::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: is_inclusive,
                    span,
                };
                continue;
            }

            // Check for binary operator
            let Some(op) = self.peek_binary_operator() else {
                break;
            };

            let precedence = op.precedence();
            if precedence < min_precedence {
                break;
            }

            // Consume operator
            self.advance();

            // Parse right-hand side with higher precedence for left-associative operators
            let next_min = if op.is_comparison() {
                precedence + 1 // non-associative comparison
            } else {
                precedence + 1 // left-associative
            };

            let right = self.parse_expression_with_precedence(next_min)?;
            let span = Span::new(left.span().start, right.span().end);
            left = Expression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parse a prefix expression (unary operators, primary literals, variables, postfix chains)
    fn parse_prefix_expression(&mut self) -> Result<Expression, ParseError> {
        let start_pos = self.current_span().start;

        let primary = match self.current().map(|t| &t.token) {
            // If expression: `if cond { a } else { b }`
            Some(Token::If) => {
                self.advance();
                let condition = self.parse_expression()?;
                let then_branch = self.parse_block()?;
                let (else_branch, end_pos) = if self.check(&Token::Else) {
                    self.advance();
                    if self.check(&Token::If) {
                        let else_if = self.parse_prefix_expression()?;
                        let end = else_if.span().end;
                        (Some(Box::new(ElseBranchExpr::ElseIf(else_if))), end)
                    } else {
                        let else_block = self.parse_block()?;
                        let end = else_block.span.end;
                        (Some(Box::new(ElseBranchExpr::Block(else_block))), end)
                    }
                } else {
                    (None, then_branch.span.end)
                };
                Expression::If {
                    condition: Box::new(condition),
                    then_branch,
                    else_branch,
                    span: Span::new(start_pos, end_pos),
                }
            }
            // Match expression: `match target { pattern => expr / block, ... }`
            Some(Token::Match) => {
                self.advance();
                let target = self.parse_expression()?;
                self.expect_token(&Token::OpenBrace, "Expected '{' after match target")?;
                let mut arms = Vec::new();
                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    let arm_start = self.current_span().start;
                    let pattern = if self.check(&Token::Underscore) {
                        let span = self.current_span();
                        self.advance();
                        Pattern::Wildcard { span }
                    } else if let Some(Token::Identifier(id)) = self.current().map(|t| &t.token) {
                        if id.as_str() == "_" {
                            let span = self.current_span();
                            self.advance();
                            Pattern::Wildcard { span }
                        } else {
                            Pattern::Expr(self.parse_expression()?)
                        }
                    } else {
                        Pattern::Expr(self.parse_expression()?)
                    };

                    self.expect_token(&Token::FatArrow, "Expected '=>' after match pattern")?;
                    let (body, arm_end) = if self.check(&Token::OpenBrace) {
                        let blk = self.parse_block()?;
                        let end = blk.span.end;
                        (MatchArmBody::Block(blk), end)
                    } else {
                        let expr = self.parse_expression()?;
                        let end = expr.span().end;
                        (MatchArmBody::Expr(expr), end)
                    };
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                    arms.push(MatchArmExpr {
                        pattern,
                        body,
                        span: Span::new(arm_start, arm_end),
                    });
                }
                let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close match")?;
                Expression::Match {
                    target: Box::new(target),
                    arms,
                    span: Span::new(start_pos, close_span.end),
                }
            }
            // Unary logical not: `not cond`
            Some(Token::Not) => {
                self.advance();
                let operand = self.parse_prefix_expression()?;
                let end_pos = operand.span().end;
                return Ok(Expression::Unary {
                    operator: UnaryOperator::Not,
                    operand: Box::new(operand),
                    span: Span::new(start_pos, end_pos),
                });
            }
            // Unary bitwise not: `~x`
            Some(Token::Tilde) => {
                self.advance();
                let operand = self.parse_prefix_expression()?;
                let end_pos = operand.span().end;
                return Ok(Expression::Unary {
                    operator: UnaryOperator::BitwiseNot,
                    operand: Box::new(operand),
                    span: Span::new(start_pos, end_pos),
                });
            }
            // Unary negation: `-x`
            Some(Token::Hyphen) => {
                self.advance();
                let operand = self.parse_prefix_expression()?;
                let end_pos = operand.span().end;
                return Ok(Expression::Unary {
                    operator: UnaryOperator::Negate,
                    operand: Box::new(operand),
                    span: Span::new(start_pos, end_pos),
                });
            }
            // Unary plus: `+x`
            Some(Token::Plus) => {
                self.advance();
                let operand = self.parse_prefix_expression()?;
                let end_pos = operand.span().end;
                return Ok(Expression::Unary {
                    operator: UnaryOperator::Plus,
                    operand: Box::new(operand),
                    span: Span::new(start_pos, end_pos),
                });
            }
            // Primary expressions
            Some(Token::Integer(n)) => {
                let val = *n;
                let span = self.current_span();
                self.advance();
                Expression::Literal { value: val, span }
            }
            Some(Token::Float(f)) => {
                let val = *f;
                let span = self.current_span();
                self.advance();
                Expression::FloatLiteral { value: val, span }
            }
            Some(Token::Measurement(m)) => {
                let val = m.value;
                let unit = m.unit.clone();
                let span = self.current_span();
                self.advance();
                Expression::Measurement {
                    value: val,
                    unit,
                    span,
                }
            }
            Some(Token::String(s)) => {
                let val = s.clone();
                let span = self.current_span();
                self.advance();
                Expression::StringLiteral { value: val, span }
            }
            Some(Token::True) => {
                let span = self.current_span();
                self.advance();
                Expression::BooleanLiteral { value: true, span }
            }
            Some(Token::False) => {
                let span = self.current_span();
                self.advance();
                Expression::BooleanLiteral { value: false, span }
            }
            Some(Token::Space) => {
                let span = self.current_span();
                self.advance();
                Expression::Variable {
                    name: "space".into(),
                    span,
                }
            }
            Some(Token::Identifier(name)) => {
                let ident_str: CompactString = name.as_str().into();
                let ident_span = self.current_span();
                self.advance();

                // Check if this is a Struct Instance expression: StructName { field: val, ... }
                if self.check(&Token::OpenBrace) && self.looks_like_struct_init() {
                    self.parse_struct_instance(ident_str, ident_span.start)?
                } else {
                    Expression::Variable {
                        name: ident_str,
                        span: ident_span,
                    }
                }
            }
            Some(Token::OpenBrace) => {
                if self.looks_like_struct_init() {
                    // Anonymous struct / map literal: `{ key: val, ... }`
                    self.parse_struct_instance("".into(), start_pos)?
                } else {
                    // Block expression: `{ let x = 1; x + 2 }`
                    let block = self.parse_block()?;
                    let span = block.span;
                    Expression::Block {
                        block,
                        span,
                    }
                }
            }
            Some(Token::OpenBracket) => {
                // Array literal: `[a, b, c]`
                self.advance(); // consume `[`
                let mut elements = Vec::new();
                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    elements.push(self.parse_expression()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' at end of array literal")?;
                Expression::ArrayLiteral {
                    elements,
                    span: Span::new(start_pos, close_span.end),
                }
            }
            Some(Token::OpenParen) => {
                self.advance(); // consume `(`
                if self.check(&Token::CloseParen) {
                    let close_span = self.expect_token(&Token::CloseParen, "Expected ')'")?;
                    Expression::Tuple {
                        elements: Vec::new(),
                        span: Span::new(start_pos, close_span.end),
                    }
                } else {
                    let first = self.parse_expression()?;
                    if self.check(&Token::Comma) {
                        // Multi-value tuple: `(e1, e2, ...)`
                        let mut elements = vec![first];
                        while self.check(&Token::Comma) {
                            self.advance();
                            if self.check(&Token::CloseParen) {
                                break;
                            }
                            elements.push(self.parse_expression()?);
                        }
                        let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close tuple")?;
                        Expression::Tuple {
                            elements,
                            span: Span::new(start_pos, close_span.end),
                        }
                    } else {
                        let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close grouped expression")?;
                        Expression::Grouped {
                            expression: Box::new(first),
                            span: Span::new(start_pos, close_span.end),
                        }
                    }
                }
            }
            Some(other) => {
                return Err(self.error(&format!("Expected expression, found {}", other)));
            }
            None => {
                return Err(self.error("Unexpected end of file while parsing expression"));
            }
        };

        // Parse postfix chain (calls, field access, indexing)
        self.parse_postfix_chain(primary)
    }

    /// Check if `{` following an identifier starts a struct instantiation rather than a block
    fn looks_like_struct_init(&self) -> bool {
        // If after `{` we have `CloseBrace` or `Identifier`/Keyword followed by `:` or `,` or `}`
        if let Some(next) = self.peek_ahead(1) {
            match &next.token {
                Token::CloseBrace => true,
                _ => {
                    if let Some(after_ident) = self.peek_ahead(2) {
                        matches!(
                            after_ident.token,
                            Token::Colon | Token::Comma | Token::CloseBrace
                        )
                    } else {
                        false
                    }
                }
            }
        } else {
            false
        }
    }

    /// Parse struct instance: `Name { field: val, field2: val2 }`
    fn parse_struct_instance(
        &mut self,
        name: CompactString,
        start_pos: usize,
    ) -> Result<Expression, ParseError> {
        self.expect_token(&Token::OpenBrace, "Expected '{' for struct instantiation")?;
        let mut fields = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let field_name_token = self.expect_identifier()?;
            let field_span_start = self.previous_span().start;
            let field_name: CompactString = field_name_token.as_str().into();

            let value = if self.check(&Token::Colon) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None // Shorthand `name`
            };

            let field_span_end = self.previous_span().end;
            fields.push(FieldInit {
                name: field_name,
                value,
                span: Span::new(field_span_start, field_span_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' at end of struct instantiation")?;
        Ok(Expression::StructInstance {
            name,
            fields,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse postfix operations: `callee(args)`, `obj.field`, `array[index]`
    fn parse_postfix_chain(&mut self, mut expr: Expression) -> Result<Expression, ParseError> {
        loop {
            if self.check(&Token::OpenParen) {
                // Function or method call: `callee(arg1, name: val)`
                self.advance(); // consume `(`
                let mut arguments = Vec::new();

                while !self.check(&Token::CloseParen) && !self.is_at_end() {
                    let arg_start = self.current_span().start;
                    // Check for named argument: `name: value` (allowing contextual keywords like from, to, type)
                    let is_named = if let Some(next) = self.peek_ahead(1) {
                        next.token == Token::Colon
                    } else {
                        false
                    };

                    let (arg_name, value) = if is_named {
                        let param_ident = self.expect_identifier()?;
                        self.expect_token(&Token::Colon, "Expected ':' after parameter name")?;
                        let val = self.parse_expression()?;
                        (Some(param_ident.name), val)
                    } else {
                        (None, self.parse_expression()?)
                    };

                    let arg_end = value.span().end;
                    arguments.push(NamedOrPositionalArg {
                        name: arg_name,
                        value,
                        span: Span::new(arg_start, arg_end),
                    });

                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }

                let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close argument list")?;
                let span = Span::new(expr.span().start, close_span.end);
                expr = Expression::Call {
                    callee: Box::new(expr),
                    arguments,
                    span,
                };
            } else if self.check(&Token::Dot) {
                // Field / Member access: `obj.field`
                self.advance(); // consume `.`
                let field_ident = self.expect_identifier()?;
                let span = Span::new(expr.span().start, self.previous_span().end);
                expr = Expression::FieldAccess {
                    target: Box::new(expr),
                    field: field_ident.as_str().into(),
                    span,
                };
            } else if self.check(&Token::OpenBracket) {
                // Index or Slice access: `target[index]` or `target[start..end]`
                self.advance(); // consume `[`
                if self.check(&Token::Range) || self.check(&Token::RangeInclusive) {
                    let is_inclusive = self.check(&Token::RangeInclusive);
                    self.advance(); // consume `..` or `..=`
                    let end_expr = if !self.check(&Token::CloseBracket) && !self.is_at_end() {
                        Some(Box::new(self.parse_expression()?))
                    } else {
                        None
                    };
                    let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close slice")?;
                    let span = Span::new(expr.span().start, close_span.end);
                    expr = Expression::Slice {
                        target: Box::new(expr),
                        start: None,
                        end: end_expr,
                        inclusive: is_inclusive,
                        span,
                    };
                } else {
                    let first_expr = self.parse_expression()?;
                    if self.check(&Token::Range) || self.check(&Token::RangeInclusive) {
                        let is_inclusive = self.check(&Token::RangeInclusive);
                        self.advance(); // consume `..` or `..=`
                        let end_expr = if !self.check(&Token::CloseBracket) && !self.is_at_end() {
                            Some(Box::new(self.parse_expression()?))
                        } else {
                            None
                        };
                        let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close slice")?;
                        let span = Span::new(expr.span().start, close_span.end);
                        expr = Expression::Slice {
                            target: Box::new(expr),
                            start: Some(Box::new(first_expr)),
                            end: end_expr,
                            inclusive: is_inclusive,
                            span,
                        };
                    } else {
                        let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close index access")?;
                        let span = Span::new(expr.span().start, close_span.end);
                        expr = Expression::Index {
                            target: Box::new(expr),
                            index: Box::new(first_expr),
                            span,
                        };
                    }
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Peek at the current token to see if it is a binary operator
    fn peek_binary_operator(&self) -> Option<BinaryOperator> {
        match self.current().map(|t| &t.token) {
            Some(Token::Or) => Some(BinaryOperator::Or),
            Some(Token::And) => Some(BinaryOperator::And),
            Some(Token::Pipe) => Some(BinaryOperator::BitwiseOr),
            Some(Token::Caret) => Some(BinaryOperator::BitwiseXor),
            Some(Token::Ampersand) => Some(BinaryOperator::BitwiseAnd),
            Some(Token::DoubleEquals) => Some(BinaryOperator::Equal),
            Some(Token::NotEquals) => Some(BinaryOperator::NotEqual),
            Some(Token::LessThan) => Some(BinaryOperator::LessThan),
            Some(Token::GreaterThan) => Some(BinaryOperator::GreaterThan),
            Some(Token::LessThanOrEqual) => Some(BinaryOperator::LessThanOrEqual),
            Some(Token::GreaterThanOrEqual) => Some(BinaryOperator::GreaterThanOrEqual),
            Some(Token::ShiftLeft) => Some(BinaryOperator::ShiftLeft),
            Some(Token::ShiftRight) => Some(BinaryOperator::ShiftRight),
            Some(Token::Plus) => Some(BinaryOperator::Add),
            Some(Token::Hyphen) => Some(BinaryOperator::Subtract),
            Some(Token::Asterisk) => Some(BinaryOperator::Multiply),
            Some(Token::Slash) => Some(BinaryOperator::Divide),
            Some(Token::Percent) => Some(BinaryOperator::Modulo),
            _ => None,
        }
    }
}
