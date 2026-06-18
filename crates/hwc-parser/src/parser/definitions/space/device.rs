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

    /// Parse nets block for net classifications (v0.1.6)
    ///
    /// Syntax:
    /// ```
    /// nets:
    ///     GND: ground
    ///     VDD: power
    ///     CLK: signal
    ///     DATA: signal, frequency: 5GHz
    /// ```
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

            // Parse classification
            let classification_str = self.expect_identifier_string()?;
            let classification = match classification_str.to_lowercase().as_str() {
                "power" => NetClassification::Power,
                "ground" | "gnd" => NetClassification::Ground,
                "signal" => NetClassification::Signal,
                _ => {
                    return Err(self.error(&format!(
                        "Invalid net classification '{}'. Expected 'power', 'ground', or 'signal'",
                        classification_str
                    )));
                }
            };

            // v0.1.7: Parse optional frequency: e.g., `signal, frequency: 5GHz`
            let mut frequency_hz: Option<f64> = None;
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
                            let unit_str = m.unit.to_string();
                            frequency_hz = Some(match unit_str.as_str() {
                                "Hz" => value,
                                "kHz" => value * 1_000.0,
                                "MHz" => value * 1_000_000.0,
                                "GHz" => value * 1_000_000_000.0,
                                _ => {
                                    return Err(self.error(&format!(
                                        "Invalid frequency unit '{}'. Expected Hz, kHz, MHz, or GHz",
                                        unit_str
                                    )));
                                }
                            });
                            self.advance(); // consume measurement token
                        } else {
                            return Err(
                                self.error("Expected frequency value with unit (e.g., 5GHz)")
                            );
                        }
                    } else {
                        return Err(self.error("Expected frequency value with unit (e.g., 5GHz)"));
                    }
                }
            }

            self.skip_whitespace();

            net_declarations.push(NetDeclaration {
                name: net_name.into(),
                classification,
                potential_mv: None,
                frequency_hz,
                span: Span::new(start_pos, self.previous_span().end),
            });
        }

        self.expect(&Token::Dedent)?;

        Ok(net_declarations)
    }
}
