use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::{span_to_source_span, ParseError};
use miette::SourceSpan;

impl crate::parser::Parser {
    /// Parse rotation: rotated 45 or rotated -30.5 or rotated 90° or rotated 90deg
    pub(in crate::parser) fn parse_rotation(&mut self) -> Result<Rotation, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Rotated)?;

        // Check for optional negative sign (v0.1.4: Hyphen is separate token)
        let is_negative = if self.check(&Token::Hyphen) {
            self.advance();
            true
        } else {
            false
        };

        // v0.1.4: Can be a standalone number or a measurement with angle unit
        let angle = if let Some(current) = self.current() {
            match &current.token {
                Token::Measurement(m) => {
                    // Check if it's an angle measurement (now Custom)
                    if let crate::lexer::units::Unit::Custom(s) = &m.unit {
                        if s == "°" || s == "deg" {
                            let val = m.value;
                            self.advance();
                            val
                        } else {
                            return Err(self.error("Expected angle measurement after 'rotated'"));
                        }
                    } else {
                        return Err(self.error("Expected angle measurement after 'rotated'"));
                    }
                }
                Token::Integer(n) => {
                    let val = *n as f64;
                    self.advance();
                    val
                }
                Token::Float(f) => {
                    let val = *f;
                    self.advance();
                    val
                }
                _ => return Err(self.error("Expected number or angle measurement after 'rotated'")),
            }
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            return Err(ParseError::UnexpectedEof { span });
        };

        // Apply negative sign if present
        let final_angle = if is_negative { -angle } else { angle };

        let end_pos = self.previous_span().end;

        Ok(Rotation {
            angle: final_angle,
            span: Span::new(start_pos, end_pos),
        })
    }
}
