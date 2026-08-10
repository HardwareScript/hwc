//! Device binding and net declaration parsing

use crate::ast::MeasurementValue;
use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;
use compact_str::CompactString;

impl crate::parser::Parser {
    /// Parse device binding: `device: DeviceName.terminal` or `device: DeviceName.termA, DeviceName.termB`
    ///
    /// Phase 4 (Silent Atom): Explicit intent-based device binding
    /// v0.2.2: Multi-terminal binding support for resistors and other 2-terminal devices
    ///
    /// # Syntax
    /// Single terminal:
    /// ```hardware
    /// add pour(Polysilicon) named Gate on z:6:
    ///     device: M1.gate
    ///     net: VIN
    /// ```
    ///
    /// Multiple terminals (resistor pattern):
    /// ```hardware
    /// add pour(Polysilicon) named Resistor_Body on layer: polyres:
    ///     device: R1.A, R1.B
    ///     net: nc
    /// ```
    ///
    /// # Returns
    /// DeviceBinding with device name and terminal names
    pub(in crate::parser) fn parse_device_binding(&mut self) -> Result<DeviceBinding, ParseError> {
        let start_pos = self.current_span().start;

        // Parse first device.terminal
        let device_name = self.expect_identifier_string()?;
        self.expect(&Token::Dot)?;
        let first_terminal = self.expect_identifier_string()?;

        let mut terminals = vec![first_terminal.into()];

        // Check for additional terminals: device: R1.A, R1.B
        while self.check(&Token::Comma) {
            self.advance(); // consume comma
            self.skip_whitespace();

            // Expect same device name
            let additional_device = self.expect_identifier_string()?;
            if additional_device != device_name {
                return Err(self.error(&format!(
                    "Multi-terminal binding must use same device name. Expected '{}', found '{}'",
                    device_name, additional_device
                )));
            }

            self.expect(&Token::Dot)?;
            let additional_terminal = self.expect_identifier_string()?;
            terminals.push(additional_terminal.into());
        }

        let end_pos = self.previous_span().end;

        // v0.2.2: Infer priority from terminal count
        let priority = crate::ast::BindingPriority::infer_from_terminals(&terminals);

        Ok(DeviceBinding {
            device_name: device_name.into(),
            terminals,
            priority,
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
            // v0.2.1: Store raw measurements, defer unit conversion to compiler phase
            let mut classification = NetClassification::Signal;
            let mut potential: Option<MeasurementValue> = None;
            let mut current: Option<MeasurementValue> = None;
            let mut frequency: Option<MeasurementValue> = None;

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
                                    let span = token.span;
                                    let value = m.value;
                                    // Extract unit string - no conversion, store as-is
                                    let unit_str = match &m.unit {
                                        crate::lexer::units::Unit::Voltage(v) => {
                                            use crate::lexer::units::VoltageUnit;
                                            match v {
                                                VoltageUnit::Volts => "V",
                                                VoltageUnit::Millivolts => "mV",
                                                VoltageUnit::Kilovolts => "kV",
                                            }
                                        }
                                        crate::lexer::units::Unit::Custom(s) => s.as_str(),
                                        _ => return Err(self.error("Expected voltage unit")),
                                    };
                                    potential = Some(MeasurementValue {
                                        value,
                                        unit: unit_str.into(),
                                        span,
                                    });
                                    self.advance();
                                }
                            }
                        }
                        "current" => {
                            if let Some(token) = self.current() {
                                if let Token::Measurement(m) = &token.token {
                                    let span = token.span;
                                    let value = m.value;
                                    // Extract unit string - no conversion, store as-is
                                    let unit_str = match &m.unit {
                                        crate::lexer::units::Unit::Current(c) => {
                                            use crate::lexer::units::CurrentUnit;
                                            match c {
                                                CurrentUnit::Amperes => "A",
                                                CurrentUnit::Milliamperes => "mA",
                                                CurrentUnit::Microamperes => "µA",
                                            }
                                        }
                                        crate::lexer::units::Unit::Custom(s) => s.as_str(),
                                        _ => return Err(self.error("Expected current unit")),
                                    };
                                    current = Some(MeasurementValue {
                                        value,
                                        unit: unit_str.into(),
                                        span,
                                    });
                                    self.advance();
                                }
                            }
                        }
                        "frequency" => {
                            if let Some(token) = self.current() {
                                if let Token::Measurement(m) = &token.token {
                                    let span = token.span;
                                    let value = m.value;
                                    // Extract unit string - no conversion, store as-is
                                    let unit_str = match &m.unit {
                                        crate::lexer::units::Unit::Custom(s) => s.as_str(),
                                        _ => return Err(self.error("Expected frequency unit")),
                                    };
                                    frequency = Some(MeasurementValue {
                                        value,
                                        unit: unit_str.into(),
                                        span,
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
                // v0.2.1: Store raw measurement value
                if self.check(&Token::Comma) {
                    self.advance(); // consume comma
                    self.skip_whitespace();
                    if self.check_identifier("frequency") {
                        self.advance(); // consume 'frequency'
                        self.expect(&Token::Colon)?;
                        self.skip_whitespace();
                        if let Some(token) = self.current() {
                            if let Token::Measurement(m) = &token.token {
                                let span = token.span;
                                let value = m.value;
                                // Extract unit string - no conversion
                                let unit_str = match &m.unit {
                                    crate::lexer::units::Unit::Custom(s) => s.as_str(),
                                    _ => return Err(self.error("Expected frequency unit")),
                                };
                                frequency = Some(MeasurementValue {
                                    value,
                                    unit: unit_str.into(),
                                    span,
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
                potential,
                current,
                frequency,
                span: Span::new(start_pos, self.previous_span().end),
            });
        }

        self.expect(&Token::Dedent)?;

        Ok(net_declarations)
    }

    /// Parse device_nets block for explicit virtual terminal bindings (v0.2.1)
    ///
    /// # Syntax
    /// ```hardware
    /// device_nets R1:
    ///     BULK: GND
    ///     SUBSTRATE: VSS
    /// device_nets M1:
    ///     bulk: AVSS
    /// ```
    ///
    /// # Returns
    /// Map of device_name -> (terminal_name -> net_name)
    pub(in crate::parser) fn parse_device_nets_block(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, rustc_hash::FxHashMap<CompactString, CompactString>>, ParseError> {
        use rustc_hash::FxHashMap;
        
        let mut device_nets: FxHashMap<CompactString, FxHashMap<CompactString, CompactString>> = FxHashMap::default();

        // Expect 'device_nets' identifier
        self.expect_identifier_string()?;
        
        // Parse device name
        let device_name = self.expect_identifier_string()?;
        
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut terminal_nets: FxHashMap<CompactString, CompactString> = FxHashMap::default();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // Parse terminal name
            let terminal_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;
            
            // Parse net name
            let net_name = self.expect_identifier_string()?;
            
            terminal_nets.insert(terminal_name.into(), net_name.into());

            self.skip_whitespace();
        }

        self.expect(&Token::Dedent)?;
        
        device_nets.insert(device_name.into(), terminal_nets);

        Ok(device_nets)
    }
}
