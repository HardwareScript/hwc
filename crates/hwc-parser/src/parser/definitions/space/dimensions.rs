//! Dimensions parsing
//!
//! v0.2.1 (Bloat Purge Category 1): `origin:` and `resolution:` are purged.
//! All spaces use the canonical Bottom-Left / Z-Up coordinate system, and
//! manufacturing snapping is governed by the PDK profile.

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse dimensions: `dimensions: 50mm by 50mm`
    ///
    /// Z-depth is intentionally absent: the board height is derived from the
    /// sum of `profile.stackup` layer thicknesses.
    pub(in crate::parser) fn parse_dimensions(&mut self) -> Result<Dimensions, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Dimensions)?;
        self.expect(&Token::Colon)?;

        let width = self.parse_measurement()?;
        self.expect(&Token::By)?;
        let height = self.parse_measurement()?;

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(Dimensions {
            width,
            height,
            span: Span::new(start_pos, end_pos),
        })
    }
}
