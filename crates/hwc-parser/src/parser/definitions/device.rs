//! Device definition parsing

use super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::ast::device::SpiceExportInfo;
use crate::lexer::{Span, Token};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

impl super::super::Parser {
    /// Parse device definition: `device NMOS:`
    ///
    /// Syntax:
    /// ```hw
    /// device NMOS:
    ///     terminals: [gate, source, drain, bulk]
    ///     materials:
    ///         gate: Polysilicon
    ///         source: Silicon_N
    ///         drain: Silicon_N
    ///         bulk: Silicon_P
    /// ```
    pub(in super::super) fn parse_device(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<DeviceDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Device) {
            collector.report(e);
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(n) => n,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }

        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            return None;
        }

        let mut terminals = None;
        let mut materials = None;
        let mut tolerance = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Parse block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "terminals" => {
                            self.advance(); // consume 'terminals'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_terminal_list() {
                                Ok(terms) => terminals = Some(terms),
                                Err(e) => {
                                    collector.report(e);
                                    self.skip_whitespace();
                                    continue;
                                }
                            }
                            self.skip_whitespace();
                            continue;
                        }
                        "materials" => {
                            self.advance(); // consume 'materials'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                continue;
                            }
                            while self.check(&Token::Newline) {
                                self.advance();
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_material_mappings() {
                                Ok(mats) => materials = Some(mats),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "tolerance" => {
                            self.advance(); // consume 'tolerance'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                continue;
                            }
                            while self.check(&Token::Newline) {
                                self.advance();
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_tolerance_mappings() {
                                Ok(tol) => tolerance = Some(tol),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            // Unknown field
            let field_name = match self.expect_identifier() {
                Ok(n) => n,
                Err(e) => {
                    collector.report(e);
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    self.skip_whitespace();
                    continue;
                }
            };

            collector.report(self.error(&format!("Unknown device field: '{}'", field_name)));
            while !self.is_at_end() && !self.check(&Token::Newline) && !self.check(&Token::Dedent) {
                self.advance();
            }
            self.skip_whitespace();
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        // Validate required fields
        let terminals = match terminals {
            Some(terms) => terms,
            None => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&Span::new(start_pos, end_pos)),
                    message: "Device definition must have 'terminals' field".into(),
                });
                return None;
            }
        };

        let materials = match materials {
            Some(mats) => mats,
            None => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&Span::new(start_pos, end_pos)),
                    message: "Device definition must have 'materials' field".into(),
                });
                return None;
            }
        };

        Some(DeviceDefinition {
            name: name.clone(),
            is_exported,
            terminals: terminals.clone(),
            materials,
            tolerance,
            spice_info: Self::generate_default_spice_info(&name, &terminals),
            span: Span::new(start_pos, end_pos),
        })
    }
    
    /// Generate default SPICE export info based on device name and terminals
    ///
    /// This provides sensible defaults for common device types. Users can override
    /// by adding explicit `spice:` blocks in future versions.
    fn generate_default_spice_info(
        device_name: &Identifier,
        terminals: &SmallVec<[CompactString; 4]>,
    ) -> Option<SpiceExportInfo> {
        let name_str = device_name.as_str();
        
        // Match common device types
        match name_str {
            "Resistor" | "PolyResistor" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("A".into());
                terminal_order.push("B".into());
                
                let mut parameters = SmallVec::new();
                parameters.push("R".into());
                
                Some(SpiceExportInfo {
                    prefix: 'R',
                    terminal_order,
                    parameters,
                    model_name: None,
                })
            }
            "Capacitor" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("Top".into());
                terminal_order.push("Bottom".into());
                
                let mut parameters = SmallVec::new();
                parameters.push("C".into());
                
                Some(SpiceExportInfo {
                    prefix: 'C',
                    terminal_order,
                    parameters,
                    model_name: None,
                })
            }
            "Inductor" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("A".into());
                terminal_order.push("B".into());
                
                let mut parameters = SmallVec::new();
                parameters.push("L".into());
                
                Some(SpiceExportInfo {
                    prefix: 'L',
                    terminal_order,
                    parameters,
                    model_name: None,
                })
            }
            "Diode" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("Anode".into());
                terminal_order.push("Cathode".into());
                
                Some(SpiceExportInfo {
                    prefix: 'D',
                    terminal_order,
                    parameters: SmallVec::new(),
                    model_name: Some("D1N4148".into()),
                })
            }
            "NMOS" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("drain".into());
                terminal_order.push("gate".into());
                terminal_order.push("source".into());
                terminal_order.push("bulk".into());
                
                let mut parameters = SmallVec::new();
                parameters.push("W".into());
                parameters.push("L".into());
                
                Some(SpiceExportInfo {
                    prefix: 'M',
                    terminal_order,
                    parameters,
                    model_name: Some("NMOS".into()),
                })
            }
            "PMOS" => {
                let mut terminal_order = SmallVec::new();
                terminal_order.push("drain".into());
                terminal_order.push("gate".into());
                terminal_order.push("source".into());
                terminal_order.push("bulk".into());
                
                let mut parameters = SmallVec::new();
                parameters.push("W".into());
                parameters.push("L".into());
                
                Some(SpiceExportInfo {
                    prefix: 'M',
                    terminal_order,
                    parameters,
                    model_name: Some("PMOS".into()),
                })
            }
            _ => {
                // For unknown device types, generate generic subcircuit format
                // X<name> <terminals in declaration order> <model_name>
                Some(SpiceExportInfo {
                    prefix: 'X',
                    terminal_order: terminals.clone(),
                    parameters: SmallVec::new(),
                    model_name: Some(device_name.to_string().into()),
                })
            }
        }
    }

    /// Parse terminal list: `[gate, source, drain, bulk]`
    fn parse_terminal_list(&mut self) -> Result<SmallVec<[CompactString; 4]>, ParseError> {
        // Expect opening bracket
        self.expect(&Token::OpenBracket)?;

        let mut terminals = SmallVec::new();

        // Parse comma-separated terminal names
        loop {
            // Skip whitespace and newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::CloseBracket) {
                break;
            }

            let terminal = self.expect_identifier_string()?;
            terminals.push(terminal.into());

            // Skip whitespace
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::Comma) {
                self.advance();
            } else if !self.check(&Token::CloseBracket) {
                return Err(self.error("Expected ',' or ']' in terminal list"));
            }
        }

        self.expect(&Token::CloseBracket)?;

        if terminals.is_empty() {
            return Err(self.error("Device must have at least one terminal"));
        }

        Ok(terminals)
    }

    fn parse_material_mappings(
        &mut self,
    ) -> Result<FxHashMap<CompactString, SmallVec<[CompactString; 2]>>, ParseError> {
        let mut mappings = FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let terminal = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;

            // Check if it's a list [Material1, Material2] or single Material
            let materials = if self.check(&Token::OpenBracket) {
                // Parse list of materials
                self.advance(); // consume '['
                let mut material_list = SmallVec::new();

                loop {
                    while self.check(&Token::Newline) {
                        self.advance();
                    }

                    if self.check(&Token::CloseBracket) {
                        break;
                    }

                    let material = self.expect_identifier_string()?;
                    material_list.push(material.into());

                    while self.check(&Token::Newline) {
                        self.advance();
                    }

                    if self.check(&Token::Comma) {
                        self.advance();
                    } else if !self.check(&Token::CloseBracket) {
                        return Err(self.error("Expected ',' or ']' in material list"));
                    }
                }

                self.expect(&Token::CloseBracket)?;

                if material_list.is_empty() {
                    return Err(self.error("Material list cannot be empty"));
                }

                material_list
            } else {
                // Single material
                let mut sv = SmallVec::new();
                sv.push(self.expect_identifier_string()?.into());
                sv
            };

            self.skip_whitespace();

            if mappings.contains_key(terminal.as_str()) {
                return Err(self.error(&format!(
                    "Duplicate material mapping for terminal '{}'",
                    terminal
                )));
            }

            mappings.insert(terminal.into(), materials);
        }

        if mappings.is_empty() {
            return Err(self.error("Device must have at least one material mapping"));
        }

        Ok(mappings)
    }

    fn parse_tolerance_mappings(&mut self) -> Result<FxHashMap<CompactString, f64>, ParseError> {
        let mut mappings = FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let param_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;

            // Parse percentage value (e.g., "1%", "5%")
            let measurement = self.parse_measurement()?;

            // Convert percentage to decimal (1% -> 0.01)
            let tolerance_value = if let crate::ast::Unit::Custom(ref unit_str) = measurement.unit {
                if unit_str == "%" {
                    measurement.value / 100.0
                } else {
                    return Err(self.error(&format!(
                        "Tolerance must be specified as percentage (e.g., '1%'), found unit: '{}'",
                        unit_str
                    )));
                }
            } else {
                return Err(self.error("Tolerance must be specified as percentage (e.g., '1%')"));
            };

            self.skip_whitespace();

            if mappings.contains_key(param_name.as_str()) {
                return Err(self.error(&format!(
                    "Duplicate tolerance specification for parameter '{}'",
                    param_name
                )));
            }

            mappings.insert(param_name.into(), tolerance_value);
        }

        if mappings.is_empty() {
            return Err(self.error("Tolerance block cannot be empty"));
        }

        Ok(mappings)
    }
}
