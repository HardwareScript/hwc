use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::ParseError;
use crate::parser::Parser;

impl Parser {
    pub fn parse_range(&mut self) -> Result<Range, ParseError> {
        let first = self.expect_integer()?;

        if self.check(&Token::RangeInclusive) {
            // Inclusive range: 7..=0
            self.advance();
            let second = self.expect_integer()?;
            Ok(Range::Slice {
                high: first,
                low: second,
                inclusive: true,
            })
        } else if self.check(&Token::Range) {
            // Exclusive range: 7..0 
            self.advance();
            let second = self.expect_integer()?;
            Ok(Range::Slice {
                high: first,
                low: second,
                inclusive: false,
            })
        } else {
            // Single bit: [5]
            Ok(Range::Single(first))
        }
    }

    pub fn parse_logic_expression(&mut self) -> Result<LogicExpression, ParseError> {
        self.parse_logic_expression_prec(0)
    }

    fn parse_logic_expression_prec(&mut self, min_prec: u8) -> Result<LogicExpression, ParseError> {
        let mut left = self.parse_logic_postfix()?;

        loop {
            if self.check(&Token::Percent) {
                let span = self.current_span();
                return Err(crate::parser::error::error_percent_as_operator(&span));
            }

            let op = match self.try_parse_logic_operator() {
                Some(op) => op,
                None => break,
            };

            let prec = op.precedence();
            if prec < min_prec {
                break;
            }

            self.advance();

            let right = self.parse_logic_expression_prec(prec + 1)?;

            let span = Span::new(left.span().start, right.span().end);

            left = LogicExpression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                span,
            };
        }

        if self.check(&Token::As) {
            self.advance();
            let target_type = self.expect_identifier_string()?;
            let span = Span::new(left.span().start, self.previous_span().end);
            left = LogicExpression::Cast {
                expression: Box::new(left),
                target_type: target_type.into(),
                span,
            };
        }

        Ok(left)
    }

    fn parse_logic_postfix(&mut self) -> Result<LogicExpression, ParseError> {
        let mut expr = self.parse_logic_primary()?;

        loop {
            if self.check(&Token::Dot) {
                self.advance();
                let field = self.expect_identifier_string()?;
                let span = Span::new(expr.span().start, self.previous_span().end);
                expr = LogicExpression::FieldAccess {
                    base: Box::new(expr),
                    field: field.into(),
                    span,
                };
            } else if self.check(&Token::OpenBracket) {
                self.advance();
                let range = self.parse_range()?;
                self.expect(&Token::CloseBracket)?;
                let span = Span::new(expr.span().start, self.previous_span().end);
                expr = LogicExpression::ArrayAccess {
                    base: Box::new(expr),
                    range,
                    span,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn try_parse_logic_operator(&self) -> Option<LogicOperator> {
        self.current().and_then(|t| match &t.token {
            Token::Plus => Some(LogicOperator::Add),
            Token::Hyphen => Some(LogicOperator::Subtract),
            Token::Asterisk => Some(LogicOperator::Multiply),
            Token::Slash => Some(LogicOperator::Divide),
            Token::Mod => Some(LogicOperator::Modulo),
            Token::Ampersand | Token::And => Some(LogicOperator::BitwiseAnd),
            Token::Pipe | Token::Or => Some(LogicOperator::BitwiseOr),
            Token::Xor => Some(LogicOperator::BitwiseXor),
            Token::ShiftLeft => Some(LogicOperator::ShiftLeft),
            Token::ShiftRight => Some(LogicOperator::ShiftRight),
            Token::Equals => Some(LogicOperator::Equal),
            Token::NotEquals => Some(LogicOperator::NotEqual),
            Token::LessThan => Some(LogicOperator::LessThan),
            Token::GreaterThan => Some(LogicOperator::GreaterThan),
            Token::LessThanOrEqual => Some(LogicOperator::LessThanOrEqual),
            Token::GreaterThanOrEqual => Some(LogicOperator::GreaterThanOrEqual),
            _ => None,
        })
    }

    fn parse_unary(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        let operator = if self.check(&Token::Exclamation) || self.check(&Token::Not) {
            self.advance();
            LogicUnaryOperator::Not
        } else {
            return Err(self.error("Expected unary operator (! or not)"));
        };

        let operand = self.parse_logic_postfix()?;
        let span = Span::new(start.start, operand.span().end);

        Ok(LogicExpression::Unary {
            operator,
            operand: Box::new(operand),
            span,
        })
    }

    fn parse_logic_primary(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        if let Some(token) = self.current() {
            match &token.token {
                Token::Exclamation | Token::Not => self.parse_unary(),

                Token::Integer(n) => {
                    let value = *n;
                    self.advance();
                    Ok(LogicExpression::Literal { value, span: start })
                }

                Token::True => {
                    self.advance();
                    Ok(LogicExpression::Boolean {
                        value: true,
                        span: start,
                    })
                }

                Token::False => {
                    self.advance();
                    Ok(LogicExpression::Boolean {
                        value: false,
                        span: start,
                    })
                }

                Token::OpenParen => {
                    self.advance();
                    let expr = self.parse_logic_expression()?;
                    self.expect(&Token::CloseParen)?;
                    let span = Span::new(start.start, self.previous_span().end);
                    Ok(LogicExpression::Grouped {
                        expression: Box::new(expr),
                        span,
                    })
                }

                Token::Match => self.parse_match_expression(),

                Token::If => self.parse_if_inline_expression(),

                Token::RegisterInit => self.parse_register_init(),

                Token::OpenBracket => self.parse_bundle_expression(),

                Token::Identifier(_) => {
                    let name = self.expect_identifier_string()?;
                    let span = Span::new(start.start, self.previous_span().end);
                    Ok(LogicExpression::Variable {
                        name: name.into(),
                        span,
                    })
                }

                _ => Err(self.error(&format!("Expected logic expression, found {}", token.token))),
            }
        } else {
            Err(self.error("Unexpected end of input in logic expression"))
        }
    }

    fn parse_match_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Match)?;

        let selector = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut arms = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            arms.push(self.parse_match_arm()?);
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::Match {
            selector,
            arms,
            span,
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.current_span();

        let pattern = if self.check(&Token::Else) {
            self.advance();
            MatchPattern::Else
        } else if let Some(Token::Integer(n)) = self.current().map(|t| &t.token) {
            let value = *n;
            self.advance();
            MatchPattern::Literal(value)
        } else {
            let enum_name = self.expect_identifier_string()?;
            self.expect(&Token::Dot)?;
            let variant = self.expect_identifier_string()?;
            MatchPattern::EnumVariant {
                enum_name: enum_name.into(),
                variant,
            }
        };

        self.expect(&Token::Colon)?;

        let body = self.parse_block_or_expr()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(MatchArm {
            pattern,
            body,
            span,
        })
    }

    fn parse_register_init(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::RegisterInit)?;
        self.expect(&Token::OpenParen)?;

        self.expect_identifier_value("clock")?;
        self.expect(&Token::Colon)?;
        let clock = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Comma)?;

        self.expect_identifier_value("reset")?;
        self.expect(&Token::Colon)?;
        let reset = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Comma)?;

        self.expect_identifier_value("init")?;
        self.expect(&Token::Colon)?;
        let init = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::CloseParen)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::RegisterInit {
            clock,
            reset,
            init,
            span,
        })
    }

    fn parse_if_inline_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;
        let condition = Box::new(self.parse_logic_expression()?);
        self.expect(&Token::Colon)?;

        let then_expr = Box::new(self.parse_block_or_expr()?);

        let bookmark = self.current;
        self.skip_whitespace();

        let else_expr = if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::Colon)?;
            Box::new(self.parse_block_or_expr()?)
        } else {
            self.current = bookmark;
            Box::new(BlockOrExpr::Expression(LogicExpression::Literal {
                value: 0,
                span: self.current_span(),
            }))
        };

        let span = Span::new(start.start, self.previous_span().end);
        Ok(LogicExpression::If {
            condition,
            then_expr,
            else_expr,
            span,
        })
    }

    fn parse_bundle_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::OpenBracket)?;

        let mut items = Vec::new();

        loop {
            self.skip_whitespace();

            if self.check(&Token::CloseBracket) {
                break;
            }

            let item_start = self.current_span();
            let expr = self.parse_logic_expression()?;

            let item = if let LogicExpression::Grouped { expression, .. } = &expr {
                if let LogicExpression::Binary {
                    left,
                    operator,
                    right,
                    ..
                } = expression.as_ref()
                {
                    if matches!(operator, LogicOperator::Multiply) {
                        if let LogicExpression::Literal { value, .. } = right.as_ref() {
                            let span = Span::new(item_start.start, self.previous_span().end);
                            BundleItem::Duplication {
                                value: left.clone(),
                                count: *value as usize,
                                span,
                            }
                        } else {
                            BundleItem::Expression(expr)
                        }
                    } else {
                        BundleItem::Expression(expr)
                    }
                } else {
                    BundleItem::Expression(expr)
                }
            } else {
                BundleItem::Expression(expr)
            };

            items.push(item);

            self.skip_whitespace();

            if !self.check(&Token::Comma) {
                break;
            }
            self.advance();
        }

        self.skip_whitespace();
        self.expect(&Token::CloseBracket)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::Bundle { items, span })
    }
}
