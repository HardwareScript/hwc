//! For loop parsing for parametric unrolling (Sprint 3.4)

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse for loop in space block (Sprint 3.4: Parametric Unrolling)
    ///
    /// Syntax:
    /// ```hardware
    /// for i in 0..8:
    ///     add Adder named Adder[i] at [x: i * 10mm, y: 0mm, z: 1]
    ///     route Adder[i].sum to Adder[i+1].carry
    /// ```
    pub(in crate::parser) fn parse_space_for_loop(&mut self) -> Result<SpaceForLoop, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::For)?;
        let variable = self.expect_identifier_string()?;
        self.expect(&Token::In)?;
        let start = self.expect_number()? as usize;
        self.expect(&Token::Range)?;
        let end = self.expect_number()? as usize;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut body = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            // Parse statements inside the for loop
            if self.check(&Token::For) {
                // Nested for loop
                let nested_loop = self.parse_space_for_loop()?;
                body.push(SpaceStatement::ForLoop(Box::new(nested_loop)));
            } else if self.check(&Token::Add) {
                // Check what kind of add statement this is
                let next_pos = self.current + 1;
                if let Some(next_token) = self.tokens.get(next_pos) {
                    match &next_token.token {
                        Token::Pour => {
                            let pour = self.parse_pour()?;
                            body.push(SpaceStatement::Pour(pour));
                        }
                        Token::Contact => {
                            let contact = self.parse_contact()?;
                            body.push(SpaceStatement::Contact(contact));
                        }
                        _ => {
                            // Component placement
                            let component = self.parse_component_placement()?;
                            body.push(SpaceStatement::Component(component));
                        }
                    }
                } else {
                    // Default to component placement
                    let component = self.parse_component_placement()?;
                    body.push(SpaceStatement::Component(component));
                }
            } else if self.check(&Token::Route) {
                let route = self.parse_route()?;
                body.push(SpaceStatement::Route(route));
            } else if self.check(&Token::Newline) {
                self.advance();
            } else {
                return Err(self.error("Expected 'add', 'route', or 'for' in for loop body"));
            }
        }

        self.expect(&Token::Dedent)?;

        Ok(SpaceForLoop {
            variable: variable.into(),
            start,
            end,
            body,
            span: Span::new(start_pos, self.previous_span().end),
        })
    }
}
