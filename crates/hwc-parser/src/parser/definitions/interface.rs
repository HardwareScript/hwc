//! Interface definition parsing (bindings, protocols)

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Interface Definition Parsing
    // ========================================================================

    /// Parse interface definition: `define interface "RobotController":`
    pub(in super::super) fn parse_interface(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<InterfaceDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Interface) {
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

        let mut target = None;
        let mut bindings = Vec::new();
        let mut protocols = Vec::new();

        // Parse interface blocks
        let mut loop_iterations = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Interface parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Interface parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Check for interface block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "bindings" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            bindings = self.parse_bindings().unwrap_or_default();
                            continue;
                        }
                        "protocols" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            protocols = self.parse_protocols().unwrap_or_default();
                            continue;
                        }
                        "target" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            // Target can be either an identifier or a string
                            target = if self.check(&Token::String("".to_string())) {
                                if let Ok(target_str) = self.expect_string() {
                                    let span = self.previous_span();
                                    Some(Identifier::new(target_str.into(), span))
                                } else {
                                    None
                                }
                            } else {
                                self.expect_identifier().ok()
                            };
                            self.skip_whitespace();
                            continue;
                        }
                        _ => {
                            let field_name = name.clone();
                            let err =
                                self.error(&format!("Unknown interface field: '{}'", field_name));
                            collector.report(err);
                            self.sync_to_next_definition();
                            continue;
                        }
                    }
                }
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Interface parser didn't advance, forcing progress");
                self.advance();
            }

            // If we get here, it's not an identifier - break
            break;
        }

        // Consume the dedent that ends the interface definition
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(InterfaceDefinition {
            name,
            is_exported,
            target,
            bindings,
            protocols,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse bindings block
    fn parse_bindings(&mut self) -> Result<Vec<Binding>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut bindings = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect: Motor_PWM = DriverIC.Pin_4
            let start_pos = self.current_span().start;
            let signal_name = self.expect_identifier_string()?;

            self.expect(&Token::Equals)?;

            let pin_ref = self.parse_pin_reference()?;
            self.skip_whitespace();

            let end_pos = self.previous_span().end;

            bindings.push(Binding {
                signal_name: signal_name.into(),
                pin_ref,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(bindings)
    }

    /// Parse protocols block
    fn parse_protocols(&mut self) -> Result<Vec<Protocol>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut protocols = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect: I2C_Bus_1:
            let start_pos = self.current_span().start;
            let protocol_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            let mut pins = Vec::new();
            let mut speed = None;

            // Parse protocol properties
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                let prop_name = self.expect_identifier()?;
                self.expect(&Token::Colon)?;

                if prop_name.as_str() == "speed" {
                    speed = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                } else {
                    // It's a pin assignment: SDA: MCU.GPIO21
                    let pin_start = self.current_span().start;
                    let pin_ref = self.parse_pin_reference()?;
                    self.skip_whitespace();
                    let pin_end = self.previous_span().end;

                    pins.push(ProtocolPin {
                        signal: prop_name.name,
                        pin_ref,
                        span: Span::new(pin_start, pin_end),
                    });
                }
            }

            if self.check(&Token::Dedent) {
                self.advance();
            }

            let end_pos = self.previous_span().end;

            protocols.push(Protocol {
                name: protocol_name.name,
                pins,
                speed,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(protocols)
    }
}
