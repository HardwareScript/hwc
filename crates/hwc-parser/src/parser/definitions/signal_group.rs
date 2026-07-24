//! Signal group definition parser

use crate::ast::{SignalGroupDefinition, SignalGroupProperty, SignalGroupType};
use crate::lexer::{Span, Token};
use crate::parser::Parser;
use rustc_hash::FxHashMap;

impl Parser {
    /// Parse signal_group definition
    ///
    /// ```hw
    /// define signal_group "USB_Data":
    ///     type: differential_pair
    ///     target_impedance: 90Ω
    ///     max_length_mismatch: 0.15mm
    /// ```
    pub(super) fn parse_signal_group_definition(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<SignalGroupDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::SignalGroup) {
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

        let mut group_type = SignalGroupType::Custom("unknown".into());
        let mut properties = FxHashMap::default();

        let mut loop_iterations = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Signal group parser infinite loop detected! Breaking."  );
                collector.report(
                    self.error(
                        "Signal group parser stuck in infinite loop - this is a compiler bug",
                    ),
                );
                break;
            }

            // Skip blank lines and comments
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            let field_name = match self.expect_identifier() {
                Ok(id) => id,
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
            };

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            match field_name.as_str() {
                "type" => {
                    let type_name = match self.expect_namespaced_identifier() {
                        Ok(id) => id,
                        Err(e) => {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                    };
                    group_type = match type_name.as_str() {
                        "differential_pair" => SignalGroupType::DifferentialPair,
                        "impedance_controlled" => SignalGroupType::ImpedanceControlled,
                        "bus" => SignalGroupType::Bus,
                        _ => SignalGroupType::Custom(type_name.name.to_string()),
                    };
                }
                "target_impedance" => {
                    // Parse impedance value (e.g., 90Ω or 90)
                    let impedance_val = if let Some(current) = self.current() {
                        match &current.token {
                            Token::Integer(val) => {
                                let v = *val as f64;
                                self.advance();
                                // Check for optional Ω unit
                                if let Some(next) = self.current() {
                                    if let Token::Identifier(unit) = &next.token {
                                        if unit == "Ω" || unit == "Ohm" || unit == "ohm" {
                                            self.advance();
                                        }
                                    }
                                }
                                v
                            }
                            Token::Float(val) => {
                                let v = *val;
                                self.advance();
                                // Check for optional Ω unit
                                if let Some(next) = self.current() {
                                    if let Token::Identifier(unit) = &next.token {
                                        if unit == "Ω" || unit == "Ohm" || unit == "ohm" {
                                            self.advance();
                                        }
                                    }
                                }
                                v
                            }
                            Token::Measurement(m) => {
                                // Handle 90Ω as a measurement token
                                let v = m.value;
                                self.advance();
                                v
                            }
                            _ => {
                                let err =
                                    self.error("Expected impedance value (number or measurement)");
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        }
                    } else {
                        let err = self.error("Expected impedance value");
                        collector.report(err);
                        self.sync_to_next_definition();
                        continue;
                    };

                    properties.insert(
                        "target_impedance".into(),
                        SignalGroupProperty::Impedance(impedance_val),
                    );
                }
                "max_length_mismatch" => {
                    let mismatch = match self.parse_measurement() {
                        Ok(m) => m,
                        Err(e) => {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                    };
                    properties.insert(
                        "max_length_mismatch".into(),
                        SignalGroupProperty::LengthMismatch(mismatch.value),
                    );
                }
                "min_spacing" => {
                    let spacing = match self.parse_measurement() {
                        Ok(m) => m,
                        Err(e) => {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                    };
                    properties.insert(
                        "min_spacing".into(),
                        SignalGroupProperty::MinSpacing(spacing.value),
                    );
                }
                "max_length" => {
                    let length = match self.parse_measurement() {
                        Ok(m) => m,
                        Err(e) => {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                    };
                    properties.insert(
                        "max_length".into(),
                        SignalGroupProperty::MaxLength(length.value),
                    );
                }
                _ => {
                    // Generic property - try to parse as string, number, or boolean
                    let property_value = if let Some(current) = self.current() {
                        match &current.token {
                            Token::String(s) => SignalGroupProperty::String(s.clone()),
                            Token::Integer(n) => SignalGroupProperty::Number(*n as f64),
                            Token::Float(f) => SignalGroupProperty::Number(*f),
                            Token::Identifier(id) if id == "true" || id == "false" => {
                                SignalGroupProperty::Boolean(id == "true")
                            }
                            Token::Identifier(id) => SignalGroupProperty::String(id.clone()),
                            _ => {
                                let err = self.error("Expected property value");
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        }
                    } else {
                        let err = self.error("Expected property value");
                        collector.report(err);
                        self.sync_to_next_definition();
                        continue;
                    };

                    self.advance();
                    properties.insert(field_name.name, property_value);
                }
            }

            if let Err(e) = self.expect(&Token::Newline) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Signal group parser didn't advance, forcing progress");
                self.advance();
            }
        }

        if let Err(e) = self.expect(&Token::Dedent) {
            collector.report(e);
        }

        let end_pos = self.previous_span().end;

        Some(SignalGroupDefinition {
            name,
            is_exported,
            group_type,
            properties,
            span: Span::new(start_pos, end_pos),
        })
    }
}
