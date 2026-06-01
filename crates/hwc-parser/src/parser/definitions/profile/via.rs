use crate::ast::*;
use crate::lexer::{Span, Token};
use super::super::super::error::{span_to_source_span, ParseError};

impl super::super::super::Parser {
    /// Parse via constraints block
    pub(super) fn parse_via_constraints(&mut self) -> Result<ViaConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut min_diameter = None;
        let mut min_annular_ring = None;
        let mut default_diameter = None;
        let mut min_spacing = None;
        let mut max_aspect_ratio = None;
        let mut default_via_fill = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "min_diameter" => {
                    min_diameter = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "min_annular_ring" => {
                    min_annular_ring = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "default_diameter" => {
                    default_diameter = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "min_spacing" => {
                    min_spacing = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_aspect_ratio" => {
                    max_aspect_ratio = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "default_via_fill" => {
                    default_via_fill = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                _ => {
                    return Err(self.error(&format!("Unknown via constraint: '{}'", field_name)));
                }
            }
        }

        let end_pos = self.previous_span().end;

        // Validate required fields
        let min_diameter = min_diameter.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'min_diameter' field".into(),
        })?;

        let min_annular_ring = min_annular_ring.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'min_annular_ring' field".into(),
        })?;

        Ok(ViaConstraints {
            min_diameter,
            min_annular_ring,
            default_diameter,
            min_spacing,
            max_aspect_ratio,
            default_via_fill,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse explicit via definition: `via Name: ...` (v0.1.7)
    pub(super) fn parse_via_definition(&mut self) -> Result<ViaDefinition, ParseError> {
        let start_pos = self.current_span().start;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut diameter = None;
        let mut annular_ring = None;
        let mut from_layer = None;
        let mut to_layer = None;
        let mut material = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "diameter" => {
                    diameter = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "annular_ring" => {
                    annular_ring = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "spanning" => {
                    // Syntax: `spanning: layer: inner2 to inner1`
                    // or just `spanning: inner2 to inner1`
                    if self.check_identifier("layer") {
                        self.advance();
                        self.expect(&Token::Colon)?;
                    }
                    from_layer = Some(self.expect_identifier()?);
                    self.expect(&Token::To)?;
                    to_layer = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                "material" => {
                    material = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                _ => {
                    return Err(self.error(&format!("Unknown via definition field: '{}'", field_name)));
                }
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        let diameter = diameter.ok_or_else(|| self.error("Via definition must include 'diameter'"))?;
        let annular_ring = annular_ring.ok_or_else(|| self.error("Via definition must include 'annular_ring'"))?;
        let from_layer = from_layer.ok_or_else(|| self.error("Via definition must include 'spanning'"))?;
        let to_layer = to_layer.ok_or_else(|| self.error("Via definition must include 'spanning'"))?;

        Ok(ViaDefinition {
            name,
            diameter,
            annular_ring,
            from_layer,
            to_layer,
            material,
            span: Span::new(start_pos, end_pos),
        })
    }
}
