use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse polygon placement: `add polygon(Copper) named WiFi_Antenna at [x:10, y:10, z:1]:`
    pub(in crate::parser) fn parse_polygon(&mut self) -> Result<PolygonPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Polygon)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.expect(&Token::At)?;
        let position = self.parse_coordinate()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut points = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            if field_name.as_str() == "points" {
                self.expect(&Token::Newline)?;
                self.expect(&Token::Indent)?;

                while !self.is_at_end() && !self.check(&Token::Dedent) {
                    if self.check(&Token::Newline) {
                        self.advance();
                        continue;
                    }

                    // Parse point: - [x, y] or - [xmm, ymm]
                    self.expect(&Token::Hyphen)?;
                    self.expect(&Token::OpenBracket)?;

                    let x = if let Some(current) = self.current() {
                        let val = match &current.token {
                            Token::Integer(n) => *n as f64,
                            Token::Float(f) => *f,
                            Token::Measurement(m) => m.value,
                            _ => {
                                return Err(
                                    self.error("Expected number or measurement for x coordinate")
                                )
                            }
                        };
                        self.advance();
                        val
                    } else {
                        return Err(self.error("Expected x coordinate"));
                    };

                    self.expect(&Token::Comma)?;

                    let y = if let Some(current) = self.current() {
                        let val = match &current.token {
                            Token::Integer(n) => *n as f64,
                            Token::Float(f) => *f,
                            Token::Measurement(m) => m.value,
                            _ => {
                                return Err(
                                    self.error("Expected number or measurement for y coordinate")
                                )
                            }
                        };
                        self.advance();
                        val
                    } else {
                        return Err(self.error("Expected y coordinate"));
                    };

                    self.expect(&Token::CloseBracket)?;
                    points.push((x, y));

                    self.expect(&Token::Newline)?;
                }

                self.expect(&Token::Dedent)?;
            } else {
                return Err(self.error(&format!("Unknown polygon property: '{}'", field_name)));
            }

            if !self.check(&Token::Dedent) {
                self.expect(&Token::Newline)?;
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(PolygonPlacement {
            material: material.into(),
            name,
            position,
            points: points.into(),
            span: Span::new(start_pos, end_pos),
        })
    }
}
