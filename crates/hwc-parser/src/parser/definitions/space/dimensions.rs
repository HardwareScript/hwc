//! Dimensions and origin parsing

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

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::DiagnosticCollector;

    fn parse_space_dimensions(source: &str) -> crate::ast::SpaceDefinition {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let program = parser.parse(&collector);
        assert!(
            !collector.has_errors(),
            "Parse errors: {}",
            collector.summary()
        );
        assert_eq!(
            program.definitions.len(),
            1,
            "Expected exactly one definition"
        );
        match program
            .definitions
            .into_iter()
            .next()
            .expect("Expected definition")
        {
            crate::ast::Definition::Space(s) => s,
            other => panic!("Expected space definition, got {:?}", other),
        }
    }

    #[test]
    fn test_resolution_parses() {
        let source = r#"space Test:
    resolution: 1nm
"#;
        let space = parse_space_dimensions(source);
        assert!(space.resolution.is_some(), "resolution should be parsed");
        let res = space.resolution.expect("resolution present");
        assert_eq!(res.value, 1.0);
        assert_eq!(res.unit, crate::ast::Unit::Nanometer);
    }
}
