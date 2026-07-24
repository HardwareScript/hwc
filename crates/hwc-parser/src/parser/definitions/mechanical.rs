//! Mechanical definition parsing (dimensions, mounting holes, keepouts)

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Mechanical Definition Parsing
    // ========================================================================

    /// Parse mechanical definition: `define mechanical "Enclosure":`
    pub(in super::super) fn parse_mechanical(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<MechanicalDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Mechanical) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let mut dimensions = None;
        let mut mounting_holes = Vec::new();
        let mut keepouts = Vec::new();

        // Parse mechanical blocks
        let mut loop_iterations = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Mechanical parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Mechanical parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Check for keyword tokens first, then fall back to identifier
            if self.check(&Token::Dimensions) {
                dimensions = self.parse_dimensions().ok();
            } else {
                let field_name = match self.expect_identifier() {
                    Ok(id) => id,
                    Err(e) => {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                };

                if let Err(e) = self.expect(&Token::Colon) {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }

                match field_name.as_str() {
                    "mounting_holes" => {
                        mounting_holes = self.parse_mounting_holes().unwrap_or_default();
                    }
                    "keepout" => {
                        keepouts = self.parse_keepouts().unwrap_or_default();
                    }
                    _ => {
                        let err =
                            self.error(&format!("Unknown mechanical field: '{}'", field_name));
                        collector.report(err);
                        self.sync_to_next_definition();
                    }
                }
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Mechanical parser didn't advance, forcing progress");
                self.advance();
            }
        }

        // Consume the dedent that ends the mechanical definition
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(MechanicalDefinition {
            name,
            is_exported,
            dimensions,
            mounting_holes,
            keepouts,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse mounting holes list
    fn parse_mounting_holes(&mut self) -> Result<Vec<MountingHole>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut holes = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect: - at [x:5, y:5] diameter 3mm (z is optional for mounting holes)
            let start_pos = self.current_span().start;
            self.expect(&Token::Hyphen)?;

            // Parse "at" keyword
            self.expect(&Token::At)?;

            // Parse position with optional z (mounting holes span all layers)
            let position = self.parse_coordinate_optional_z()?;

            // Parse "diameter"
            let diameter_keyword = self.expect_identifier()?;
            if diameter_keyword.as_str() != "diameter" {
                return Err(self.error(&format!(
                    "Expected 'diameter', found '{}'",
                    diameter_keyword
                )));
            }

            let diameter = self.parse_measurement()?;
            self.skip_whitespace();

            let end_pos = self.previous_span().end;

            holes.push(MountingHole {
                position,
                diameter,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(holes)
    }

    /// Parse keepout regions list
    fn parse_keepouts(&mut self) -> Result<Vec<Keepout>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut keepouts = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect: - region [x:20, y:20] to [x:60, y:60] height 15mm
            let start_pos = self.current_span().start;
            self.expect(&Token::Hyphen)?;

            // Parse "region"
            let region_keyword = self.expect_identifier()?;
            if region_keyword.as_str() != "region" {
                return Err(self.error(&format!("Expected 'region', found '{}'", region_keyword)));
            }

            let from = self.parse_coordinate()?;
            self.expect(&Token::To)?;
            let to = self.parse_coordinate()?;

            // Parse optional "height"
            let height = if let Some(current_token) = self.current() {
                if let Token::Identifier(id) = &current_token.token {
                    if id == "height" {
                        self.advance();
                        Some(self.parse_measurement()?)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            self.skip_whitespace();
            let end_pos = self.previous_span().end;

            keepouts.push(Keepout {
                from,
                to,
                height,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(keepouts)
    }
}
