//! SPICE Model Card Parser (v0.2.1+)
//!
//! Parses SPICE model definitions that declare semiconductor physics parameters.
//!
//! Syntax:
//! ```hw
//! export spice_model DMOD:
//!     type: diode
//!     parameters:
//!         IS: 1e-12
//!         N: 1.0
//!         RS: 0.1
//! ```
//!
//! Philosophy:
//! - NO hardcoded parameter names (IS, VTO, etc. are just identifiers)
//! - NO defaults (empty parameters block is an error)
//! - NO unwrapped (all required fields must be present)
//! - Fail loudly with clear messages

use crate::ast::*;
use crate::lexer::Token;
use crate::parser::error::{span_to_source_span, ParseError};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

impl super::super::Parser {
    /// Parse spice_model definition: `spice_model DMOD:` or `export spice_model DMOD:`
    ///
    /// Reports errors to collector and returns None if parsing fails.
    pub(in super::super) fn parse_spice_model(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<SpiceModelDefinition> {
        let start_pos = self.current_span().start;

        // Expect 'spice_model' keyword
        if let Err(e) = self.expect(&Token::SpiceModel) {
            collector.report(e);
            return None;
        }

        // Parse model name
        let name = match self.expect_identifier() {
            Ok(n) => n,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        self.skip_whitespace();

        // Expect indent for body
        if let Err(_e) = self.expect(&Token::Indent) {
            collector.report(ParseError::ExpectedIndent {
                span: span_to_source_span(&self.current_span()),
                message: format!("spice_model '{}' body", name.name).into(),
            });
            return None;
        }

        let mut model_type: Option<CompactString> = None;
        let mut parameters: Option<FxHashMap<CompactString, f64>> = None;

        // Parse body properties
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Parse property name
            let prop_name = match self.expect_identifier_string() {
                Ok(n) => n,
                Err(e) => {
                    collector.report(e);
                    // Skip to next line and try to continue
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    continue;
                }
            };

            // Expect colon
            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                // Skip to next line and try to continue
                while !self.is_at_end()
                    && !self.check(&Token::Newline)
                    && !self.check(&Token::Dedent)
                {
                    self.advance();
                }
                continue;
            }

            self.skip_whitespace();

            match prop_name.as_str() {
                "type" => {
                    // Parse model type (identifier: diode, nmos, pmos, etc.)
                    match self.expect_identifier_string() {
                        Ok(t) => {
                            model_type = Some(t.into());
                        }
                        Err(e) => {
                            collector.report(e);
                            // Skip to next line and try to continue
                            while !self.is_at_end()
                                && !self.check(&Token::Newline)
                                && !self.check(&Token::Dedent)
                            {
                                self.advance();
                            }
                            continue;
                        }
                    }
                }
                "parameters" => {
                    // Parse parameters block
                    self.skip_whitespace();

                    if let Err(_e) = self.expect(&Token::Indent) {
                        collector.report(ParseError::ExpectedIndent {
                            span: span_to_source_span(&self.current_span()),
                            message: format!("spice_model '{}' parameters block", name.name).into(),
                        });
                        // Skip to next line and try to continue
                        while !self.is_at_end()
                            && !self.check(&Token::Newline)
                            && !self.check(&Token::Dedent)
                        {
                            self.advance();
                        }
                        continue;
                    }

                    let mut params = FxHashMap::default();

                    // Parse parameter key-value pairs
                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                        self.skip_whitespace();

                        if self.check(&Token::Dedent) || self.is_at_end() {
                            break;
                        }

                        // Parse parameter name
                        let param_name = match self.expect_identifier_string() {
                            Ok(n) => n,
                            Err(e) => {
                                collector.report(e);
                                // Skip to next line and try to continue
                                while !self.is_at_end()
                                    && !self.check(&Token::Newline)
                                    && !self.check(&Token::Dedent)
                                {
                                    self.advance();
                                }
                                continue;
                            }
                        };

                        // Expect colon
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            // Skip to next line and try to continue
                            while !self.is_at_end()
                                && !self.check(&Token::Newline)
                                && !self.check(&Token::Dedent)
                            {
                                self.advance();
                            }
                            continue;
                        }

                        self.skip_whitespace();

                        // Parse parameter value (number)
                        let value = match self.parse_numeric_value() {
                            Ok(v) => v,
                            Err(e) => {
                                collector.report(e);
                                // Skip to next line and try to continue
                                while !self.is_at_end()
                                    && !self.check(&Token::Newline)
                                    && !self.check(&Token::Dedent)
                                {
                                    self.advance();
                                }
                                continue;
                            }
                        };

                        params.insert(param_name.into(), value);
                        self.skip_whitespace();
                    }

                    // Expect dedent after parameters block
                    if let Err(e) = self.expect(&Token::Dedent) {
                        collector.report(e);
                    }

                    parameters = Some(params);
                }
                _ => {
                    collector.report(ParseError::UnknownField {
                        span: span_to_source_span(&self.current_span()),
                        message: format!(
                            "Unknown field '{}' in spice_model '{}'. Expected 'type' or 'parameters'",
                            prop_name, name.name
                        ).into(),
                    });
                    // Skip to next line and try to continue
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    continue;
                }
            }

            self.skip_whitespace();
        }

        // Expect dedent after body
        if let Err(e) = self.expect(&Token::Dedent) {
            collector.report(e);
        }

        let end_pos = self.previous_span().end;
        let span = crate::lexer::Span::new(start_pos, end_pos);

        // REQUIRED FIELD VALIDATION - NO DEFAULTS!

        // Validate: type is required
        let model_type = match model_type {
            Some(t) => t,
            None => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&span),
                    message: format!(
                        "spice_model '{}' missing REQUIRED field 'type'. Add 'type: diode', 'type: nmos', etc.",
                        name.name
                    ).into(),
                });
                return None;
            }
        };

        // Validate: parameters is required
        let parameters = match parameters {
            Some(p) => p,
            None => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&span),
                    message: format!(
                        "spice_model '{}' missing REQUIRED field 'parameters'. Add 'parameters:' block with at least one parameter",
                        name.name
                    ).into(),
                });
                return None;
            }
        };

        // Use SpiceModelDefinition::new for validation (empty params check)
        match SpiceModelDefinition::new(name, model_type, parameters, is_exported, span) {
            Ok(model) => Some(model),
            Err(err_msg) => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&span),
                    message: err_msg.into(),
                });
                None
            }
        }
    }

    /// Parse a numeric value (integer or float)
    ///
    /// Handles scientific notation (1e-12) and standard decimals (3.14)
    fn parse_numeric_value(&mut self) -> Result<f64, ParseError> {
        // Handle optional sign
        let sign = if self.check(&Token::Hyphen) {
            self.advance();
            -1.0
        } else if self.check(&Token::Plus) {
            self.advance();
            1.0
        } else {
            1.0
        };

        match self.current() {
            Some(t) if matches!(&t.token, Token::Float(_)) => {
                if let Token::Float(f) = &t.token {
                    let value = *f * sign;
                    self.advance();
                    Ok(value)
                } else {
                    unreachable!()
                }
            }
            Some(t) if matches!(&t.token, Token::Integer(_)) => {
                if let Token::Integer(i) = &t.token {
                    let value = (*i as f64) * sign;
                    self.advance();
                    Ok(value)
                } else {
                    unreachable!()
                }
            }
            Some(t) => Err(ParseError::UnexpectedToken {
                span: span_to_source_span(&t.span),
                expected: "numeric value (integer or float)".into(),
                found: format!("{}", t.token).into(),
            }),
            None => Err(ParseError::UnexpectedEof {
                span: if let Some(last) = self.tokens.last() {
                    span_to_source_span(&last.span)
                } else {
                    miette::SourceSpan::new(0.into(), 0.into())
                },
            }),
        }
    }
}
