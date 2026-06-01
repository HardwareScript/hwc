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

            self.skip_whitespace();

            net_declarations.push(NetDeclaration {
                name: net_name.into(),
                classification,
                potential_mv: None, // TODO: Parse optional voltage in future
                span: Span::new(start_pos, self.previous_span().end),
            });
        }

        self.expect(&Token::Dedent)?;

        Ok(net_declarations)
    }
}
