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
            if self.check(&Token::Newline) {
                self.advance();
            } else {
                body.push(self.parse_space_statement()?);
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

    /// Parse compile-time if conditional in space block (v0.2.1)
    ///
    /// This is NOT runtime control flow - it's a compile-time code generator condition.
    /// The condition is evaluated during loop unrolling to generate different geometry.
    ///
    /// Syntax:
    /// ```hardware
    /// if (row + col) mod 2 == 0:
    ///     add plane(Aluminum) named L1_R{row}_C{col} on layer: metal1
    /// else:
    ///     add plane(Tungsten) named L1_R{row}_C{col} on layer: metal1
    /// ```
    pub(in crate::parser) fn parse_space_if_conditional(
        &mut self,
    ) -> Result<SpaceIfConditional, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::If)?;
        let condition = self.parse_expression()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut then_body = Vec::new();

        // Parse then block
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            then_body.push(self.parse_space_statement()?);
        }

        self.expect(&Token::Dedent)?;

        // Parse optional else block
        let mut else_body = Vec::new();
        if self.check(&Token::Else) {
            self.advance(); // consume 'else'
            self.expect(&Token::Colon)?;
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            while !self.is_at_end() && !self.check(&Token::Dedent) {
                self.skip_whitespace();

                if self.check(&Token::Dedent) {
                    break;
                }

                else_body.push(self.parse_space_statement()?);
            }

            self.expect(&Token::Dedent)?;
        }

        Ok(SpaceIfConditional {
            condition,
            then_body,
            else_body,
            span: Span::new(start_pos, self.previous_span().end),
        })
    }

    /// Helper to parse a single space statement (used by both for and if)
    fn parse_space_statement(&mut self) -> Result<SpaceStatement, ParseError> {
        if self.check(&Token::For) {
            let nested_loop = self.parse_space_for_loop()?;
            let for_id = self.arena.alloc_for_loop(nested_loop);
            Ok(SpaceStatement::ForLoop(for_id))
        } else if self.check(&Token::If) {
            let if_stmt = self.parse_space_if_conditional()?;
            Ok(SpaceStatement::If(if_stmt))
        } else if self.check(&Token::Let) {
            // v0.2.1: Loop-scoped let bindings
            let let_binding = self.parse_space_let_binding()?;
            Ok(SpaceStatement::Let(let_binding))
        } else if self.check(&Token::Add) {
            // Check what kind of add statement this is
            let next_pos = self.current + 1;
            if let Some(next_token) = self.tokens.get(next_pos) {
                match &next_token.token {
                    Token::Pour => {
                        let pour = self.parse_pour()?;
                        let pour_id = self.arena.alloc_pour(pour);
                        Ok(SpaceStatement::Pour(pour_id))
                    }
                    Token::Plane => {
                        let plane = self.parse_plane()?;
                        let plane_id = self.arena.alloc_plane(plane);
                        Ok(SpaceStatement::Plane(plane_id))
                    }
                    Token::Polygon => {
                        let polygon = self.parse_polygon()?;
                        let polygon_id = self.arena.alloc_polygon(polygon);
                        Ok(SpaceStatement::Polygon(polygon_id))
                    }
                    Token::Contact => {
                        let contact_id = self.parse_contact()?;
                        Ok(SpaceStatement::Contact(contact_id))
                    }
                    _ => {
                        // Component placement
                        let component = self.parse_component_placement()?;
                        let comp_id = self.arena.alloc_component(component);
                        Ok(SpaceStatement::Component(comp_id))
                    }
                }
            } else {
                // Default to component placement
                let component = self.parse_component_placement()?;
                let comp_id = self.arena.alloc_component(component);
                Ok(SpaceStatement::Component(comp_id))
            }
        } else if self.check(&Token::Route) {
            let route = self.parse_route()?;
            // Arena-allocate for SoC-scale performance
            let route_id = self.arena.alloc_route(route);
            Ok(SpaceStatement::Route(route_id))
        } else if self.check(&Token::Newline) {
            self.advance();
            self.parse_space_statement() // Recurse to get next real statement
        } else {
            Err(self.error("Expected 'add', 'route', 'if', 'for', or 'let' in space block"))
        }
    }
}
