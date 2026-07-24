//! Shape definition parsing (points, geometry, CSG, generators)
//!
//! This module is organized into logical submodules:
//! - `parameters`: Shape parameter parsing
//! - `points`: Shape points parsing
//! - `generator`: Procedural shape generator parsing
//! - `geometry`: Mode B parametric geometry block parsing + geometry expression parser
//! - `csg`: Mode C CSG expression parsing
//! - `helpers`: Expression string reading utilities

mod csg;
mod generator;
mod geometry;
mod helpers;
mod parameters;
mod points;

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::{span_to_source_span, ParseError};

impl crate::parser::Parser {
    pub(super) fn parse_shape(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<ShapeDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Shape) {
            collector.report(e);
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(n) => n,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        let parameters = if self.check(&Token::OpenParen) {
            self.advance();
            let params = match self.parse_shape_parameters() {
                Ok(p) => p,
                Err(e) => {
                    collector.report(e);
                    return None;
                }
            };
            if let Err(e) = self.expect(&Token::CloseParen) {
                collector.report(e);
                return None;
            }
            params
        } else {
            Vec::new()
        };

        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }
        self.skip_whitespace();
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            return None;
        }

        let mut points = Vec::new();
        let mut generator: Option<ShapeGenerator> = None;
        let mut geometry: Option<Vec<GeometryBlock>> = None;
        let mut csg: Option<CsgExpression> = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if let Some(current) = self.current() {
                if let Token::Identifier(field_name) = &current.token {
                    if field_name == "points" {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            return None;
                        }
                        if let Err(e) = self.expect(&Token::Newline) {
                            collector.report(e);
                            return None;
                        }
                        self.skip_whitespace();
                        if let Err(e) = self.expect(&Token::Indent) {
                            collector.report(e);
                            return None;
                        }
                        match self.parse_shape_points() {
                            Ok(pts) => points = pts,
                            Err(e) => {
                                collector.report(e);
                                while !self.check(&Token::Dedent) && !self.is_at_end() {
                                    self.advance();
                                }
                            }
                        }
                        if self.check(&Token::Dedent) {
                            self.advance();
                        }
                        continue;
                    }

                    if field_name == "geometry" {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            return None;
                        }
                        if let Err(e) = self.expect(&Token::Newline) {
                            collector.report(e);
                            return None;
                        }
                        self.skip_whitespace();
                        if let Err(e) = self.expect(&Token::Indent) {
                            collector.report(e);
                            return None;
                        }
                        if self.check_identifier("Rectangle")
                            || self.check_identifier("Circle")
                            || self.check_csg_operator()
                        {
                            match self.parse_csg_expression() {
                                Ok(expr) => csg = Some(expr),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else if self.check_identifier("let") && self.lookahead_is_csg_let() {
                            match self.parse_csg_with_let_bindings() {
                                Ok(expr) => csg = Some(expr),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else if self.check_identifier("for")
                            || self.check_identifier("let")
                            || self.check_identifier("Point")
                        {
                            match self.parse_geometry_blocks() {
                                Ok(blocks) => geometry = Some(blocks),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else {
                            match self.parse_shape_generator() {
                                Ok(gen) => generator = Some(gen),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        }
                        if self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::Dedent) {
                            self.advance();
                        }
                        continue;
                    }
                }
            }

            let field_name = match self.expect_identifier() {
                Ok(n) => n,
                Err(e) => {
                    collector.report(e);
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    self.skip_whitespace();
                    continue;
                }
            };
            collector.report(self.error(&format!("Unknown shape field: '{}'", field_name)));
            while !self.is_at_end() && !self.check(&Token::Newline) && !self.check(&Token::Dedent) {
                self.advance();
            }
            self.skip_whitespace();
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        if points.is_empty() && generator.is_none() && geometry.is_none() && csg.is_none() {
            collector.report(ParseError::General {
                span: span_to_source_span(&Span::new(start_pos, end_pos)),
                message: "Shape definition must have 'points', 'geometry' generator, geometry blocks, or CSG expression".into(),
            });
            return None;
        }

        Some(ShapeDefinition {
            name,
            is_exported,
            parameters,
            points,
            generator,
            geometry,
            csg,
            span: Span::new(start_pos, end_pos),
        })
    }
}
