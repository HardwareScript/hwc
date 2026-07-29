use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse coordinate: [X,Y,Z] (positional) or [x:10, y:15, z:2] (declarative)
    pub(in crate::parser) fn parse_coordinate(&mut self) -> Result<Coordinate, ParseError> {
        self.parse_coordinate_with_optional_z(false)
    }

    /// Parse coordinate with optional z (for mechanical features that span all layers)
    pub(in crate::parser) fn parse_coordinate_optional_z(
        &mut self,
    ) -> Result<Coordinate, ParseError> {
        self.parse_coordinate_with_optional_z(true)
    }

    /// Parse coordinate with configurable z requirement
    ///
    /// Supports three syntaxes:
    /// 1. Positional: `[X, Y, Z]`
    /// 2. Declarative: `[x:10mm, y:15mm, z:1]`
    /// 3. Relative (v0.1.6): `AnchorName.edge + offset` or `last.edge + offset`
    fn parse_coordinate_with_optional_z(
        &mut self,
        z_optional: bool,
    ) -> Result<Coordinate, ParseError> {
        // Relative positioning syntax: AnchorName.edge + offset or last.edge + offset
        // Relative positioning ONLY occurs when the current token does NOT start with '['
        if !self.check(&Token::OpenBracket) {
            if let Some(token) = self.current() {
                // Check for 'last' or 'substrate' keyword (special anchors)
                if matches!(token.token, Token::Last | Token::Substrate) {
                    return self.parse_relative_coordinate();
                }

                if matches!(token.token, Token::Identifier(_)) {
                    // Quick check: is the NEXT token a dot or open bracket?
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        match &next_token.token {
                            Token::Dot => {
                                // Simple case: Anchor.edge
                                return self.parse_relative_coordinate();
                            }
                            Token::OpenBracket => {
                                // Possible array syntax: Name[...].edge
                                let mut lookahead_pos = self.current + 2;
                                let mut bracket_depth = 1;

                                while bracket_depth > 0 && lookahead_pos < self.tokens.len() {
                                    if let Some(t) = self.tokens.get(lookahead_pos) {
                                        match t.token {
                                            Token::OpenBracket => bracket_depth += 1,
                                            Token::CloseBracket => bracket_depth -= 1,
                                            _ => {}
                                        }
                                    }
                                    lookahead_pos += 1;
                                }

                                if let Some(after_bracket) = self.tokens.get(lookahead_pos) {
                                    if matches!(after_bracket.token, Token::Dot) {
                                        return self.parse_relative_coordinate();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Not relative positioning, expect bracket for absolute coordinates
        self.expect(&Token::OpenBracket)?;

        // Check if this is declarative syntax by looking for identifier followed by colon
        let is_declarative = if let Some(token) = self.current() {
            matches!(token.token, Token::Identifier(_))
                && self
                    .tokens
                    .get(self.current + 1)
                    .is_some_and(|t| matches!(t.token, Token::Colon))
        } else {
            false
        };

        if is_declarative {
            self.parse_declarative_coordinate_impl(z_optional)
        } else {
            self.parse_positional_coordinate(z_optional)
        }
    }

    /// Parse relative coordinate: AnchorName.edge + offset or last.edge + offset
    ///
    /// Syntax:
    /// - `M1.right + 1mm` - Single measurement offset
    /// - `M1.top + [0.5mm, 1mm, 0mm]` - Vector offset
    /// - `last.right + 1mm` - Reference to previous loop iteration (v0.1.6)
    ///
    /// Edges: left, right, top, bottom, front, back
    fn parse_relative_coordinate(&mut self) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;

        // Parse anchor name (may be 'last' keyword or identifier with optional array syntax)
        let anchor_name = if self.check(&Token::Last) {
            self.advance(); // consume 'last'
            "last".to_string()
        } else if self.check(&Token::Substrate) {
            self.advance(); // consume 'substrate'
            "substrate".to_string()
        } else {
            self.parse_anchor_name()?
        };
        let anchor_span = self.previous_span();

        // Expect dot
        self.expect(&Token::Dot)?;

        // Parse edge name
        let edge_str = self.expect_identifier_string()?;
        let edge = match edge_str.as_str() {
            "left" => Edge::Left,
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            "front" => Edge::Front,
            "back" => Edge::Back,
            "min_z" => Edge::MinZ,
            "max_z" => Edge::MaxZ,
            "top_left" => Edge::TopLeft,
            "top_right" => Edge::TopRight,
            "bottom_left" => Edge::BottomLeft,
            "bottom_right" => Edge::BottomRight,
            "center" => Edge::Center,
            _ => {
                return Err(self.error(&format!(
                    "Invalid edge '{}'. Expected: left, right, top, bottom, front, back, min_z, max_z, top_left, top_right, bottom_left, bottom_right, or center",
                    edge_str
                )))
            }
        };

        // Optional: Expect '+' or '-' and offset
        let offset = if self.check(&Token::Plus) || self.check(&Token::Hyphen) {
            let is_subtraction = self.check(&Token::Hyphen);
            self.advance(); // consume '+' or '-'

            // Parse offset: either single measurement or vector [x, y, z] or [x, y]
            if self.check(&Token::OpenBracket) {
                // Vector offset: [x, y, z] or [x, y]
                self.advance(); // consume '['

                let x = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let y = self.parse_expression()?;

                let z = if self.check(&Token::Comma) {
                    self.advance(); // consume ','
                    self.parse_expression()?
                } else {
                    let end_pos = self.current_span().start;
                    Expression::Measurement {
                        value: 0.0,
                        unit: crate::ast::Unit::Millimeter,
                        span: Span::new(end_pos, end_pos),
                    }
                };

                self.expect(&Token::CloseBracket)?;

                // If subtraction, negate the values
                let (x, y, z) = if is_subtraction {
                    (
                        Expression::Unary {
                            operator: UnaryOperator::Negate,
                            operand: Box::new(x),
                            span: Span::new(start_pos, self.previous_span().end),
                        },
                        Expression::Unary {
                            operator: UnaryOperator::Negate,
                            operand: Box::new(y),
                            span: Span::new(start_pos, self.previous_span().end),
                        },
                        Expression::Unary {
                            operator: UnaryOperator::Negate,
                            operand: Box::new(z),
                            span: Span::new(start_pos, self.previous_span().end),
                        },
                    )
                } else {
                    (x, y, z)
                };

                RelativeOffset::Vector { x, y, z }
            } else {
                // Single measurement offset
                let measurement = self.parse_measurement()?;
                if is_subtraction {
                    // Negate single measurement
                    RelativeOffset::Single(crate::ast::Measurement {
                        value: -measurement.value,
                        unit: measurement.unit,
                        span: measurement.span,
                    })
                } else {
                    RelativeOffset::Single(measurement)
                }
            }
        } else {
            // Default to zero offset
            RelativeOffset::Single(crate::ast::Measurement {
                value: 0.0,
                unit: crate::ast::Unit::Millimeter,
                span: Span::new(self.previous_span().end, self.previous_span().end),
            })
        };

        let end_pos = self.previous_span().end;

        Ok(Coordinate::Relative(RelativePosition {
            anchor: AnchorReference {
                name: anchor_name.into(),
                span: anchor_span,
            },
            edge,
            offset,
            span: Span::new(start_pos, end_pos),
        }))
    }

    /// Parse anchor name with optional array syntax
    ///
    /// Supports:
    /// - Simple names: `M1`, `Resistor`
    /// - Array syntax: `Adder[0]`, `Component[i-1]`, `Block[i*2]`
    ///
    /// Returns the full anchor name as a string (e.g., "Adder[i-1]")
    pub(in crate::parser) fn parse_anchor_name(&mut self) -> Result<String, ParseError> {
        let base_name = self.expect_identifier_string()?;

        // Check for array syntax: Name[expr]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['

            // Parse the index expression
            // We need to collect tokens until we find the matching ']'
            let mut index_str = String::new();
            let mut bracket_depth = 1;

            while bracket_depth > 0 && !self.is_at_end() {
                if let Some(spanned_token) = self.current() {
                    match &spanned_token.token {
                        Token::OpenBracket => {
                            index_str.push('[');
                            bracket_depth += 1;
                            self.advance();
                        }
                        Token::CloseBracket => {
                            bracket_depth -= 1;
                            if bracket_depth > 0 {
                                index_str.push(']');
                            }
                            self.advance();
                        }
                        Token::Identifier(name) => {
                            index_str.push_str(name);
                            self.advance();
                        }
                        Token::Integer(n) => {
                            index_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Plus => {
                            index_str.push('+');
                            self.advance();
                        }
                        Token::Hyphen => {
                            index_str.push('-');
                            self.advance();
                        }
                        Token::Asterisk => {
                            index_str.push('*');
                            self.advance();
                        }
                        Token::Slash => {
                            index_str.push('/');
                            self.advance();
                        }
                        _ => {
                            return Err(self.error(&format!(
                                "Unexpected token in array index: {}",
                                spanned_token.token
                            )));
                        }
                    }
                } else {
                    break;
                }
            }

            if bracket_depth != 0 {
                return Err(self.error("Unclosed bracket in array index"));
            }

            Ok(format!("{}[{}]", base_name, index_str))
        } else {
            Ok(base_name)
        }
    }

    /// Parse positional coordinate: [X, Y, Z] (or [X, Y] if z_optional)
    fn parse_positional_coordinate(
        &mut self,
        z_optional: bool,
    ) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;
        let x = self.parse_expression()?; // X first
        self.expect(&Token::Comma)?;
        let y = self.parse_expression()?; // Y second

        let z = if z_optional && self.check(&Token::CloseBracket) {
            let end_pos = self.current_span().start;
            Expression::Measurement {
                value: 0.0,
                unit: crate::ast::Unit::Millimeter,
                span: Span::new(end_pos, end_pos),
            }
        } else {
            self.expect(&Token::Comma)?;
            self.parse_expression()? // Z third
        };

        self.expect(&Token::CloseBracket)?;
        let end_pos = self.previous_span().end;

        Ok(Coordinate::Positional {
            x,
            y,
            z,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse declarative coordinate with optional z
    fn parse_declarative_coordinate_impl(
        &mut self,
        z_optional: bool,
    ) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;
        let mut x = None;
        let mut y = None;
        let mut z = None;

        // Parse first coordinate pair
        self.parse_coordinate_pair(&mut x, &mut y, &mut z)?;

        // Parse remaining coordinate pairs
        while self.check(&Token::Comma) {
            self.advance(); // consume comma
            self.parse_coordinate_pair(&mut x, &mut y, &mut z)?;
        }

        self.expect(&Token::CloseBracket)?;
        let end_pos = self.previous_span().end;

        // Ensure required coordinates were specified
        let x = x.ok_or_else(|| self.error("Missing 'x' coordinate in declarative syntax"))?;
        let y = y.ok_or_else(|| self.error("Missing 'y' coordinate in declarative syntax"))?;

        let z = if z_optional {
            z.unwrap_or_else(|| Expression::Measurement {
                value: 0.0,
                unit: crate::ast::Unit::Millimeter,
                span: Span::new(start_pos, end_pos),
            })
        } else {
            z.ok_or_else(|| self.error("Missing 'z' coordinate in declarative syntax"))?
        };

        Ok(Coordinate::Declarative {
            x,
            y,
            z,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse a single coordinate pair: x:10, y:15, or z:2
    fn parse_coordinate_pair(
        &mut self,
        x: &mut Option<Expression>,
        y: &mut Option<Expression>,
        z: &mut Option<Expression>,
    ) -> Result<(), ParseError> {
        let axis = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        let value = self.parse_expression()?;

        match axis.as_str() {
            "x" => {
                if x.is_some() {
                    return Err(self.error("Duplicate 'x' coordinate"));
                }
                *x = Some(value);
            }
            "y" => {
                if y.is_some() {
                    return Err(self.error("Duplicate 'y' coordinate"));
                }
                *y = Some(value);
            }
            "z" => {
                if z.is_some() {
                    return Err(self.error("Duplicate 'z' coordinate"));
                }
                *z = Some(value);
            }
            _ => {
                return Err(self.error(&format!(
                    "Invalid coordinate axis '{}'. Expected 'x', 'y', or 'z' (lowercase only)",
                    axis
                )))
            }
        }

        Ok(())
    }
}
