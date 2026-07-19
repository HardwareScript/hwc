//! Device binding and net declaration parsing

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse device binding: `device: DeviceName.terminal`
    ///
    /// Phase 4 (Silent Atom): Explicit intent-based device binding
    ///
    /// # Syntax
    /// ```hardware
    /// add pour(Polysilicon) named Gate on z:6:
    ///     device: M1.gate
    ///     net: VIN
    ///     boundary: [x: 400um, y: 400um] to [x: 600um, y: 1400um]
    /// ```
    ///
    /// # Returns
    /// DeviceBinding with device name and terminal name
    pub(in crate::parser) fn parse_device_binding(&mut self) -> Result<DeviceBinding, ParseError> {
        let start_pos = self.current_span().start;

        // Parse device name (e.g., "M1")
        let device_name = self.expect_identifier_string()?;

        // Expect dot separator
        self.expect(&Token::Dot)?;

        // Parse terminal name (e.g., "gate", "source", "drain", "bulk")
        let terminal = self.expect_identifier_string()?;

        let end_pos = self.previous_span().end;

        Ok(DeviceBinding {
            device_name: device_name.into(),
            terminal: terminal.into(),
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse net
    pub(in crate::parser) fn parse_nets_block(
        &mut self,
    ) -> Result<Vec<NetDeclaration>, ParseError> {
        let start_pos = self.current_span().start;

        // Expect 'nets' identifier
        self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut net_declarations = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // Parse net name
            let net_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;

            // v0.1.8: Support both simple classification and brace-enclosed properties
            let mut classification = NetClassification::Signal;
            let mut potential_mv: Option<i64> = None;
            let mut current_ma: Option<f64> = None;
            let mut frequency_hz: Option<f64> = None;

            if self.check(&Token::OpenBrace) {
                self.advance(); // consume '{'
                while !self.is_at_end() && !self.check(&Token::CloseBrace) {
                    let key = self.expect_identifier_string()?;
                    self.expect(&Token::Colon)?;
                    match key.as_str() {
                        "classification" => {
                            let val = self.expect_identifier_string()?;
                            classification = match val.to_lowercase().as_str() {
                                "power" => NetClassification::Power,
                                "ground" | "gnd" => NetClassification::Ground,
                                "signal" => NetClassification::Signal,
                                "highvoltage" => NetClassification::HighVoltage,
                                _ => NetClassification::Unclassified,
                            };
                        }
                        "potential" => {
                            if let Some(token) = self.current() {
                                if let Token::Measurement(m) = &token.token {
                                    let value = m.value;
                                    use crate::lexer::units::{Unit as LexerUnit, VoltageUnit};
                                    potential_mv = Some(match m.unit {
                                        LexerUnit::Voltage(VoltageUnit::Volts) => {
                                            (value * 1000.0) as i64
                                        }
                                        LexerUnit::Voltage(VoltageUnit::Millivolts) => value as i64,
                                        _ => {
                                            return Err(self.error("Expected V or mV for potential"))
                                        }
                                    });
                                    self.advance();
                                }
                            }
                        }
                        "current" => {
                            if let Some(token) = self.current() {
                                if let Token::Measurement(m) = &token.token {
                                    let value = m.value;
                                    use crate::lexer::units::{CurrentUnit, Unit as LexerUnit};
                                    current_ma = Some(match m.unit {
                                        LexerUnit::Current(CurrentUnit::Amperes) => value * 1000.0,
                                        LexerUnit::Current(CurrentUnit::Milliamperes) => value,
                                        LexerUnit::Current(CurrentUnit::Microamperes) => {
                                            value / 1000.0
                                        }
                                        _ => {
                                            return Err(
                                                self.error("Expected A, mA, or uA for current")
                                            )
                                        }
                                    });
                                    self.advance();
                                }
                            }
                        }
                        "frequency" => {
                            if let Some(token) = self.current() {
                                if let Token::Measurement(m) = &token.token {
                                    let value = m.value;
                                    use crate::lexer::units::Unit as LexerUnit;
                                    frequency_hz = Some(match m.unit {
                                        LexerUnit::Custom(ref s) if s == "Hz" => value,
                                        LexerUnit::Custom(ref s) if s == "kHz" => value * 1_000.0,
                                        LexerUnit::Custom(ref s) if s == "MHz" => {
                                            value * 1_000_000.0
                                        }
                                        LexerUnit::Custom(ref s) if s == "GHz" => {
                                            value * 1_000_000_000.0
                                        }
                                        _ => {
                                            return Err(self.error("Expected Hz, kHz, MHz, or GHz"))
                                        }
                                    });
                                    self.advance();
                                }
                            }
                        }
                        _ => {
                            self.advance();
                        } // Skip unknown
                    }
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(&Token::CloseBrace)?;
            } else {
                // Legacy simple classification: `GND: ground`
                let classification_str = self.expect_identifier_string()?;
                classification = match classification_str.to_lowercase().as_str() {
                    "power" => NetClassification::Power,
                    "ground" | "gnd" => NetClassification::Ground,
                    "signal" => NetClassification::Signal,
                    _ => NetClassification::Unclassified,
                };

                // v0.1.7: Parse optional frequency: e.g., `signal, frequency: 5GHz`
                if self.check(&Token::Comma) {
                    self.advance(); // consume comma
                    self.skip_whitespace();
                    if self.check_identifier("frequency") {
                        self.advance(); // consume 'frequency'
                        self.expect(&Token::Colon)?;
                        self.skip_whitespace();
                        if let Some(token) = self.current() {
                            if let Token::Measurement(m) = &token.token {
                                let value = m.value;
                                use crate::lexer::units::Unit as LexerUnit;
                                frequency_hz = Some(match m.unit {
                                    LexerUnit::Custom(ref s) if s == "Hz" => value,
                                    LexerUnit::Custom(ref s) if s == "kHz" => value * 1_000.0,
                                    LexerUnit::Custom(ref s) if s == "MHz" => value * 1_000_000.0,
                                    LexerUnit::Custom(ref s) if s == "GHz" => {
                                        value * 1_000_000_000.0
                                    }
                                    _ => return Err(self.error("Invalid frequency unit")),
                                });
                                self.advance();
                            }
                        }
                    }
                }
            }

            self.skip_whitespace();

            net_declarations.push(NetDeclaration {
                name: net_name.into(),
                classification,
                potential_mv,
                current_ma,
                frequency_hz,
                span: Span::new(start_pos, self.previous_span().end),
            });
        }

        self.expect(&Token::Dedent)?;

        Ok(net_declarations)
    }
}
