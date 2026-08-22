use crate::ast::{BinaryOperator, Expression, Span, UnaryOperator};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse an expression using Pratt parsing (operator precedence)
    /// This handles: literals, variables, binary ops (+, -, *, /, %), unary ops (-, +), and parentheses
    pub(super) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_expression_with_precedence(0)
    }

    /// Parse expression with minimum precedence (Pratt parser)
    fn parse_expression_with_precedence(
        &mut self,
        min_precedence: u8,
    ) -> Result<Expression, ParseError> {
        // Parse the left-hand side (prefix expression)
        let mut left = self.parse_prefix_expression()?;

        // Parse binary operators with precedence climbing
        loop {
            // Check for invalid % operator usage before checking for valid operators
            if let Some(token) = self.current() {
                if matches!(token.token, Token::Percent) {
                    return Err(self.error(
                        "The '%' symbol is only valid as a unit suffix (e.g., 50%). For modulo operations, use the 'mod' keyword instead."
                    ));
                }
            }

            let Some(op) = self.peek_binary_operator() else {
                break;
            };

            let precedence = op.precedence();

            if precedence < min_precedence {
                break;
            }

            // Consume the operator token
            self.advance();

            // Parse the right-hand side with higher precedence
            let right = self.parse_expression_with_precedence(precedence + 1)?;

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

    /// Parse a prefix expression (unary operators, literals, variables, grouped expressions)
    pub(super) fn parse_prefix_expression(&mut self) -> Result<Expression, ParseError> {
        let start_pos = self.current_span().start;

        match self.current() {
            Some(token) => match &token.token {
                // Unary minus: -x
                Token::Hyphen => {
                    self.advance();
                    let operand = self.parse_prefix_expression()?;
                    let end_pos = operand.span().end;
                    Ok(Expression::Unary {
                        operator: UnaryOperator::Negate,
                        operand: Box::new(operand),
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Unary plus: +x
                Token::Plus => {
                    self.advance();
                    let operand = self.parse_prefix_expression()?;
                    let end_pos = operand.span().end;
                    Ok(Expression::Unary {
                        operator: UnaryOperator::Plus,
                        operand: Box::new(operand),
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Unary not: not x (v0.2.1)
                Token::Not => {
                    self.advance();
                    let operand = self.parse_prefix_expression()?;
                    let end_pos = operand.span().end;
                    Ok(Expression::Unary {
                        operator: UnaryOperator::Not,
                        operand: Box::new(operand),
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Grouped expression: (expr)
                Token::OpenParen => {
                    self.advance();
                    let expression = self.parse_expression()?;
                    self.expect(&Token::CloseParen)?;
                    let end_pos = self.previous_span().end;
                    Ok(Expression::Grouped {
                        expression: Box::new(expression),
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Coordinate literal: [x, y, z] or [x, y]
                // v0.2.0: Coordinates are now first-class expressions
                Token::OpenBracket => {
                    let coord = self.parse_coordinate_optional_z()?;
                    let end_pos = self.previous_span().end;
                    Ok(Expression::Coordinate {
                        coord: Box::new(coord),
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Integer literal
                Token::Integer(value) => {
                    let value = *value;
                    let span = token.span;
                    self.advance();
                    Ok(Expression::Literal { value, span })
                }
                // Float literal (v0.1.7)
                Token::Float(value) => {
                    let value = *value;
                    let span = token.span;
                    self.advance();
                    Ok(Expression::FloatLiteral { value, span })
                }
                // Measurement literal (e.g., 0.3mm, 2.5mm, 50%)
                Token::Measurement(_) => {
                    let measurement = self.parse_measurement()?;
                    let span = self.previous_span();

                    // SMART PARSER: If the unit is "%", treat it as a Percentage expression
                    // This allows coordinates like [x: 50%, y: 50%, z: 1]
                    // while keeping electrical properties like tolerance: 1% as measurements
                    if measurement.unit == super::Unit::Custom("%".into()) {
                        Ok(Expression::Percentage {
                            value: measurement.value,
                            span,
                        })
                    } else {
                        Ok(Expression::Measurement {
                            value: measurement.value,
                            unit: measurement.unit,
                            span,
                        })
                    }
                }
                // Identifier or variable
                Token::Identifier(_name) => {
                    // Check if this is a function call: name(args)
                    let is_function_call = if let Some(next) = self.peek_ahead(1) {
                        matches!(next.token, Token::OpenParen)
                    } else {
                        false
                    };

                    if is_function_call {
                        // Parse function call: sin(x), cos(angle), etc.
                        let func_name = self.expect_identifier_string()?;
                        let func_span = self.previous_span();

                        self.expect(&Token::OpenParen)?;

                        // Parse comma-separated arguments
                        let mut arguments = Vec::new();

                        if !self.check(&Token::CloseParen) {
                            loop {
                                arguments.push(self.parse_expression()?);

                                if !self.check(&Token::Comma) {
                                    break;
                                }
                                self.advance(); // consume comma
                            }
                        }

                        self.expect(&Token::CloseParen)?;
                        let end_pos = self.previous_span().end;

                        return Ok(Expression::FunctionCall {
                            name: func_name.into(),
                            arguments,
                            span: Span::new(func_span.start, end_pos),
                        });
                    }

                    // Check if this is an anchor reference: ComponentName.edge, Instance.SubEntity.edge, or ComponentName[i].edge
                    let is_anchor = {
                        let mut lookahead = 0;
                        let mut found_anchor = false;

                        while let Some(t) = self.tokens.get(self.current + lookahead) {
                            if matches!(t.token, Token::Identifier(_)) {
                                lookahead += 1;
                                // Skip optional array brackets: [i] or [0]
                                if let Some(tok_bracket) = self.tokens.get(self.current + lookahead) {
                                    if matches!(tok_bracket.token, Token::OpenBracket) {
                                        let mut depth = 1;
                                        lookahead += 1;
                                        while depth > 0 && self.current + lookahead < self.tokens.len() {
                                            if let Some(bt) = self.tokens.get(self.current + lookahead) {
                                                match bt.token {
                                                    Token::OpenBracket => depth += 1,
                                                    Token::CloseBracket => depth -= 1,
                                                    _ => {}
                                                }
                                            }
                                            lookahead += 1;
                                        }
                                    }
                                }

                                // Must be followed by a dot
                                if let Some(tok_dot) = self.tokens.get(self.current + lookahead) {
                                    if matches!(tok_dot.token, Token::Dot) {
                                        lookahead += 1;
                                        // Check if the next token is an edge name
                                        if let Some(next_tok) = self.tokens.get(self.current + lookahead) {
                                            if let Token::Identifier(next_id) = &next_tok.token {
                                                let is_edge = matches!(
                                                    next_id.as_str(),
                                                    "left"
                                                        | "right"
                                                        | "top"
                                                        | "bottom"
                                                        | "front"
                                                        | "back"
                                                        | "min_z"
                                                        | "max_z"
                                                        | "top_left"
                                                        | "top_right"
                                                        | "bottom_left"
                                                        | "bottom_right"
                                                        | "center"
                                                        | "center_x"
                                                        | "center_y"
                                                        | "center_z"
                                                );
                                                if is_edge {
                                                    // Check if there's another dot after this edge name
                                                    if let Some(after_edge) =
                                                        self.tokens.get(self.current + lookahead + 1)
                                                    {
                                                        if !matches!(after_edge.token, Token::Dot) {
                                                            found_anchor = true;
                                                            break;
                                                        }
                                                    } else {
                                                        found_anchor = true;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        found_anchor
                    };

                    if is_anchor {
                        let mut segments = Vec::new();
                        segments.push(self.parse_anchor_name()?);

                        while self.check(&Token::Dot) {
                            // Check if what follows Dot is an edge name and NOT followed by another Dot
                            if let Some(next_tok) = self.tokens.get(self.current + 1) {
                                if let Token::Identifier(next_id) = &next_tok.token {
                                    let is_edge = matches!(
                                        next_id.as_str(),
                                        "left"
                                            | "right"
                                            | "top"
                                            | "bottom"
                                            | "front"
                                            | "back"
                                            | "min_z"
                                            | "max_z"
                                            | "top_left"
                                            | "top_right"
                                            | "bottom_left"
                                            | "bottom_right"
                                            | "center"
                                            | "center_x"
                                            | "center_y"
                                            | "center_z"
                                    );
                                    if is_edge {
                                        if let Some(after) = self.tokens.get(self.current + 2) {
                                            if !matches!(after.token, Token::Dot) {
                                                break;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                            self.advance(); // consume Dot
                            segments.push(self.parse_anchor_name()?);
                        }

                        self.expect(&Token::Dot)?;

                        // Parse edge name
                        let edge_str = self.expect_identifier_string()?;
                        let edge = match edge_str.as_str() {
                            "left" => crate::ast::Edge::Left,
                            "right" => crate::ast::Edge::Right,
                            "top" => crate::ast::Edge::Top,
                            "bottom" => crate::ast::Edge::Bottom,
                            "front" => crate::ast::Edge::Front,
                            "back" => crate::ast::Edge::Back,
                            "min_z" => crate::ast::Edge::MinZ,
                            "max_z" => crate::ast::Edge::MaxZ,
                            "top_left" => crate::ast::Edge::TopLeft,
                            "top_right" => crate::ast::Edge::TopRight,
                            "bottom_left" => crate::ast::Edge::BottomLeft,
                            "bottom_right" => crate::ast::Edge::BottomRight,
                            "center" => crate::ast::Edge::Center,
                            // v0.2.1: Comptime anchor arithmetic properties
                            "center_x" => crate::ast::Edge::CenterX,
                            "center_y" => crate::ast::Edge::CenterY,
                            "center_z" => crate::ast::Edge::CenterZ,
                            _ => {
                                return Err(self.error(&format!(
                                    "Invalid edge '{}'. Expected: left, right, top, bottom, front, back, min_z, max_z, top_left, top_right, bottom_left, bottom_right, center, center_x, center_y, or center_z",
                                    edge_str
                                )))
                            }
                        };

                        let anchor_name = segments.join(".");
                        let end_pos = self.previous_span().end;
                        Ok(Expression::AnchorReference {
                            anchor: crate::ast::AnchorReference {
                                name: anchor_name.into(),
                                span: Span::new(start_pos, end_pos),
                            },
                            edge,
                            span: Span::new(start_pos, end_pos),
                        })
                    } else {
                        // Just a variable reference (supports namespaced member access like pdk.edge_clearance)
                        let name = self.expect_namespaced_identifier_string()?;
                        let span = self.previous_span();
                        Ok(Expression::Variable {
                            name: name.into(),
                            span,
                        })
                    }
                }
                // v0.2.0: Handle 'space' keyword as a special anchor reference: space.bottom_left
                Token::Space => {
                    let span = token.span;
                    self.advance();

                    // Expect dot
                    self.expect(&Token::Dot)?;

                    // Parse edge name
                    let edge_str = self.expect_identifier_string()?;
                    let edge = match edge_str.as_str() {
                        "left" => crate::ast::Edge::Left,
                        "right" => crate::ast::Edge::Right,
                        "top" => crate::ast::Edge::Top,
                        "bottom" => crate::ast::Edge::Bottom,
                        "front" => crate::ast::Edge::Front,
                        "back" => crate::ast::Edge::Back,
                        "min_z" => crate::ast::Edge::MinZ,
                        "max_z" => crate::ast::Edge::MaxZ,
                        "top_left" => crate::ast::Edge::TopLeft,
                        "top_right" => crate::ast::Edge::TopRight,
                        "bottom_left" => crate::ast::Edge::BottomLeft,
                        "bottom_right" => crate::ast::Edge::BottomRight,
                        "center" => crate::ast::Edge::Center,
                        _ => {
                            return Err(self.error(&format!(
                            "Invalid space anchor '{}'. Expected: left, right, top, bottom, top_left, top_right, bottom_left, bottom_right, center",
                            edge_str
                        )))
                        }
                    };

                    let end_pos = self.previous_span().end;
                    Ok(Expression::AnchorReference {
                        anchor: crate::ast::AnchorReference {
                            name: "space".into(),
                            span,
                        },
                        edge,
                        span: Span::new(start_pos, end_pos),
                    })
                }
                // Anchor reference with 'substrate' keyword: substrate.edge
                Token::Substrate => {
                    let span = token.span;
                    self.advance();

                    // Expect dot
                    self.expect(&Token::Dot)?;

                    // Parse edge name
                    let edge_str = self.expect_identifier_string()?;
                    let edge = match edge_str.as_str() {
                        "left" => crate::ast::Edge::Left,
                        "right" => crate::ast::Edge::Right,
                        "top" => crate::ast::Edge::Top,
                        "bottom" => crate::ast::Edge::Bottom,
                        "front" => crate::ast::Edge::Front,
                        "back" => crate::ast::Edge::Back,
                        "min_z" => crate::ast::Edge::MinZ,
                        "max_z" => crate::ast::Edge::MaxZ,
                        "top_left" => crate::ast::Edge::TopLeft,
                        "top_right" => crate::ast::Edge::TopRight,
                        "bottom_left" => crate::ast::Edge::BottomLeft,
                        "bottom_right" => crate::ast::Edge::BottomRight,
                        "center" => crate::ast::Edge::Center,
                        _ => {
                            return Err(self.error(&format!(
                            "Invalid edge '{}'. Expected: left, right, top, bottom, front, back, min_z, or max_z",
                            edge_str
                        )))
                        }
                    };

                    let end_pos = self.previous_span().end;
                    Ok(Expression::AnchorReference {
                        anchor: crate::ast::AnchorReference {
                            name: "substrate".into(),
                            span,
                        },
                        edge,
                        span: Span::new(start_pos, end_pos),
                    })
                }
                _ => Err(self.error(&format!(
                    "Expected expression, found {}",
                    self.token_description(&token.token)
                ))),
            },
            None => Err(self.error("Expected expression, found end of file")),
        }
    }

    /// Peek at the current token and return the binary operator if it is one
    fn peek_binary_operator(&self) -> Option<BinaryOperator> {
        self.current().and_then(|token| match &token.token {
            Token::Plus => Some(BinaryOperator::Add),
            Token::Hyphen => Some(BinaryOperator::Subtract),
            Token::Asterisk => Some(BinaryOperator::Multiply),
            Token::Slash => Some(BinaryOperator::Divide),
            Token::Mod => Some(BinaryOperator::Modulo), // v0.2.1: 'mod' keyword for modulo
            // Token::Percent is NOT a valid operator - it's only valid as a unit suffix
            // The modulo operation uses the 'mod' keyword instead
            // v0.2.1: Comparison operators for compile-time conditionals
            // v0.1.6 ARENA Update: Single '=' is used for both assignment and comparison
            Token::Equals => Some(BinaryOperator::Equal),
            Token::NotEquals => Some(BinaryOperator::NotEqual),
            Token::LessThan => Some(BinaryOperator::LessThan),
            Token::GreaterThan => Some(BinaryOperator::GreaterThan),
            Token::LessThanOrEqual => Some(BinaryOperator::LessThanOrEqual),
            Token::GreaterThanOrEqual => Some(BinaryOperator::GreaterThanOrEqual),
            // v0.2.1: Boolean operators for compile-time conditionals
            Token::And => Some(BinaryOperator::And),
            Token::Or => Some(BinaryOperator::Or),
            _ => None,
        })
    }
}
