//! Unit definition and measurement parsing

use super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};
use miette::SourceSpan;

impl super::super::Parser {
    // ========================================================================
    // Measurement Parsing
    // ========================================================================

    /// Parse measurement token (v0.1.4: measurements are atomic tokens from lexer)
    /// v0.1.6: Supports optional negative sign prefix
    pub(in super::super) fn parse_measurement(&mut self) -> Result<Measurement, ParseError> {
        let is_negative = if self.check(&Token::Hyphen) {
            self.advance();
            true
        } else {
            false
        };

        if let Some(current) = self.current() {
            if let Token::Measurement(m) = &current.token {
                let span = current.span;
                let value = if is_negative { -m.value } else { m.value };
                let unit = m.unit.clone();
                self.advance();

                // Convert lexer::units::Unit to ast::Unit
                let ast_unit = Self::convert_lexer_unit_to_ast(&unit)?;

                Ok(Measurement {
                    value,
                    unit: ast_unit,
                    span,
                })
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "measurement".into(),
                    found: format!("{}", current.token).into(),
                })
            }
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            Err(ParseError::UnexpectedEof { span })
        }
    }

    /// Convert lexer unit to AST unit
    fn convert_lexer_unit_to_ast(unit: &crate::lexer::units::Unit) -> Result<Unit, ParseError> {
        use crate::lexer::units::{CurrentUnit, DistanceUnit, TemperatureUnit, VoltageUnit};

        Ok(match unit {
            crate::lexer::units::Unit::Distance(u) => match u {
                DistanceUnit::Millimeters => Unit::Millimeter,
                DistanceUnit::Centimeters => Unit::Centimeter,
                DistanceUnit::Micrometers => Unit::Micrometer,
            },
            crate::lexer::units::Unit::Voltage(u) => match u {
                VoltageUnit::Volts => Unit::Volt,
                VoltageUnit::Millivolts => Unit::Millivolt,
                VoltageUnit::Kilovolts => Unit::Kilovolt,
            },
            crate::lexer::units::Unit::Current(u) => match u {
                CurrentUnit::Amperes => Unit::Ampere,
                CurrentUnit::Milliamperes => Unit::Milliampere,
                CurrentUnit::Microamperes => Unit::Microampere,
            },
            crate::lexer::units::Unit::Temperature(u) => match u {
                TemperatureUnit::Celsius => Unit::Celsius,
            },
            // Custom units (%, ppm, mAh, dBm, Ω, F, H, Hz, etc.) - pass through as strings
            crate::lexer::units::Unit::Custom(s) => Unit::Custom(s.clone()),
        })
    }

    // ========================================================================
    // Unit Definition Parsing (Standard Library)
    // ========================================================================

    /// Parse unit definition: `define unit "Microfarad":`
    pub(in super::super) fn parse_unit(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<UnitDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Unit) {
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

        let mut symbol = None;
        let mut aliases = Vec::new();
        let mut base_si = None;
        let mut multiplier = None;
        let mut dimension = None;
        let mut description = None;
        let mut note = None;
        let mut examples = Vec::new();

        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Unit parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Unit parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            // Parse property key (accept keywords as keys)
            // v0.1.6: Property keys are just identifiers
            let key_str = if let Some(spanned) = self.current() {
                if let Token::Identifier(id) = &spanned.token {
                    let key = id.clone();
                    self.advance();
                    key
                } else {
                    break;
                }
            } else {
                break;
            };

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            // Parse value based on key
            match key_str.as_str() {
                "symbol" => {
                    symbol = self.expect_string().ok().map(Into::into);
                }
                "aliases" => {
                    // Parse array of strings: ["uF", "microF"]
                    if let Err(e) = self.expect(&Token::OpenBracket) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                        if let Some(spanned) = self.current() {
                            if let Token::String(s) = &spanned.token {
                                aliases.push(s.clone().into());
                                self.advance();
                            }
                        }
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    if let Err(e) = self.expect(&Token::CloseBracket) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                }
                "base_si" => {
                    // base_si references another unit's symbol, so it's a string
                    base_si = self.expect_string().ok().map(Into::into);
                }
                "multiplier" => {
                    // Parse number (float or scientific notation)
                    if let Some(spanned) = self.current() {
                        match &spanned.token {
                            Token::Float(f) => {
                                multiplier = Some(*f);
                                self.advance();
                            }
                            Token::Integer(i) => {
                                multiplier = Some(*i as f64);
                                self.advance();
                            }
                            _ => {}
                        }
                    }
                }
                "dimension" => {
                    // v0.1.6: dimension is a bare identifier (e.g., length not "length")
                    dimension = self.expect_identifier().ok().map(|id| id.name);
                }
                "description" => {
                    description = self.expect_string().ok();
                }
                "note" => {
                    note = self.expect_string().ok().map(Into::into);
                }
                "examples" => {
                    // Parse array of strings
                    if let Err(e) = self.expect(&Token::OpenBracket) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                        if let Some(spanned) = self.current() {
                            if let Token::String(s) = &spanned.token {
                                examples.push(s.clone().into());
                                self.advance();
                            }
                        }
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    if let Err(e) = self.expect(&Token::CloseBracket) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                }
                _ => {
                    // Skip unknown properties
                    self.advance();
                }
            }

            self.skip_whitespace();

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Unit parser didn't advance, forcing progress");
                self.advance();
            }
        }

        if let Err(e) = self.expect(&Token::Dedent) {
            collector.report(e);
        }
        let end_pos = self.previous_span().end;

        // Validate required fields
        let symbol: Option<String> = match symbol {
            Some(s) => s,
            None => {
                let err = self.error("Unit definition missing 'symbol' field");
                collector.report(err);
                return None;
            }
        };

        let dimension = match dimension {
            Some(d) => d,
            None => {
                let err = self.error("Unit definition missing 'dimension' field");
                collector.report(err);
                return None;
            }
        };

        Some(UnitDefinition {
            name,
            symbol: symbol
                .map(|s: String| s.into())
                .unwrap_or_else(|| "".into()),
            aliases,
            base_si,
            multiplier,
            dimension,
            description: description.map(|s: String| s.into()),
            note,
            examples,
            span: Span::new(start_pos, end_pos),
        })
    }
}
