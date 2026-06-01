//! Dimensions, grid, and origin parsing

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
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

    /// Parse grid: `grid: 500 by 500 by 4`
    pub(in crate::parser) fn parse_grid(&mut self) -> Result<Grid, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Grid)?;
        self.expect(&Token::Colon)?;

        let x = self.expect_integer()?;
        self.expect(&Token::By)?;
        let y = self.expect_integer()?;
        self.expect(&Token::By)?;
        let z = self.expect_integer()?;

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(Grid {
            x,
            y,
            z,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse origin: `origin: tl by t` or `origin: tl` (defaults to `tl by t`)
    pub(in crate::parser) fn parse_origin(&mut self) -> Result<OriginPoint, ParseError> {
        self.expect(&Token::Origin)?;
        self.expect(&Token::Colon)?;

        // Parse XY component
        let xy = match self.current().map(|t| &t.token) {
            Some(Token::TopLeft) => {
                self.advance();
                OriginXY::TL
            }
            Some(Token::BottomLeft) => {
                self.advance();
                OriginXY::BL
            }
            Some(Token::TopRight) => {
                self.advance();
                OriginXY::TR
            }
            Some(Token::BottomRight) => {
                self.advance();
                OriginXY::BR
            }
            _ => return Err(self.error("Expected origin XY point (tl, bl, tr, or br)")),
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
}
