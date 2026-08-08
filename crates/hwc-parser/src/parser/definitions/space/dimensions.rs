//! Dimensions and origin parsing

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse dimensions: `dimensions: 50mm by 50mm by 4mm`
    pub(in crate::parser) fn parse_dimensions(&mut self) -> Result<Dimensions, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Dimensions)?;
        self.expect(&Token::Colon)?;

        let width = self.parse_measurement()?;
        self.expect(&Token::By)?;
        let height = self.parse_measurement()?;
        self.expect(&Token::By)?;
        let depth = self.parse_measurement()?;

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(Dimensions {
            width,
            height,
            depth,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse origin: `origin: tl by t` or `origin: tl` (defaults to `tl by t`)
    pub(in crate::parser) fn parse_origin(&mut self) -> Result<OriginPoint, ParseError> {
        self.expect(&Token::Origin)?;
        self.expect(&Token::Colon)?;

        // Parse XY component
        let xy = if self.check_identifier("tl") {
            self.advance();
            OriginXY::TL
        } else if self.check_identifier("bl") {
            self.advance();
            OriginXY::BL
        } else if self.check_identifier("tr") {
            self.advance();
            OriginXY::TR
        } else if self.check_identifier("br") {
            self.advance();
            OriginXY::BR
        } else {
            return Err(self.error("Expected origin XY point (tl, bl, tr, or br)"));
        };

        // Check for optional Z component: `by t` or `by b`
        let z = if self.check(&Token::By) {
            self.advance(); // consume 'by'

            match self.current().map(|t| &t.token) {
                Some(Token::Identifier(id)) if id == "t" => {
                    self.advance();
                    OriginZ::Top
                }
                Some(Token::Identifier(id)) if id == "b" => {
                    self.advance();
                    OriginZ::Bottom
                }
                _ => {
                    return Err(
                        self.error("Expected Z-axis direction (t for top-down, b for bottom-up)")
                    )
                }
            }
        } else {
            // Default to top-down if Z not specified
            OriginZ::Top
        };

        self.skip_whitespace();

        Ok(OriginPoint { xy, z })
    }

    /// Parse resolution: `resolution: 1nm`
    ///
    /// Specifies the minimum feature size for the space.
    pub(in crate::parser) fn parse_resolution(&mut self) -> Result<Measurement, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Resolution)?;
        self.expect(&Token::Colon)?;

        let resolution = self.parse_measurement()?;

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        // Ensure the span covers the full parsed expression
        let _ = (start_pos, end_pos);

        Ok(resolution)
    }
}
