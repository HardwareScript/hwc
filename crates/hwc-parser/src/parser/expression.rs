//! HardwareScript v0.3.0 Pratt Expression Parser

use crate::ast::{
    BinaryOperator, Expression, FieldInit, NamedOrPositionalArg, Span, UnaryOperator,
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
                // Anonymous struct / map literal: `{ key: val, ... }`
                self.parse_struct_instance("".into(), start_pos)?
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
                // Grouped expression: `(expr)`
                self.advance();
                let inner = self.parse_expression()?;
                let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close grouped expression")?;
                Expression::Grouped {
                    expression: Box::new(inner),
                    span: Span::new(start_pos, close_span.end),
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
                // Index access: `target[index]`
                self.advance(); // consume `[`
                let index_expr = self.parse_expression()?;
                let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close index access")?;
                let span = Span::new(expr.span().start, close_span.end);
                expr = Expression::Index {
                    target: Box::new(expr),
                    index: Box::new(index_expr),
                    span,
                };
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
            Some(Token::DoubleEquals) => Some(BinaryOperator::Equal),
            Some(Token::NotEquals) => Some(BinaryOperator::NotEqual),
            Some(Token::LessThan) => Some(BinaryOperator::LessThan),
            Some(Token::GreaterThan) => Some(BinaryOperator::GreaterThan),
            Some(Token::LessThanOrEqual) => Some(BinaryOperator::LessThanOrEqual),
            Some(Token::GreaterThanOrEqual) => Some(BinaryOperator::GreaterThanOrEqual),
            Some(Token::Plus) => Some(BinaryOperator::Add),
            Some(Token::Hyphen) => Some(BinaryOperator::Subtract),
            Some(Token::Asterisk) => Some(BinaryOperator::Multiply),
            Some(Token::Slash) => Some(BinaryOperator::Divide),
            Some(Token::Percent) => Some(BinaryOperator::Modulo),
            _ => None,
        }
    }
}
