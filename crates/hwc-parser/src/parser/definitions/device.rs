//! Device definition parsing

use super::super::error::{span_to_source_span, ParseError};
use crate::ast::device::SpiceExportInfo;
use crate::ast::*;
use crate::lexer::{Span, Token};
use compact_str::CompactString;
use rustc_hash::{FxHashMap, FxHashSet};
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
        let mut metrics = None;
        let mut spice_info = None;

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
                        "metrics" => {
                            self.advance(); // consume 'metrics'
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
                            match self.parse_metrics_mappings() {
                                Ok(m) => metrics = Some(m),
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
                        "spice" => {
                            self.advance(); // consume 'spice'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_spice_block() {
                                Ok(info) => spice_info = Some(info),
                                Err(e) => {
                                    collector.report(e);
                                    self.skip_whitespace();
                                    continue;
                                }
                            }
                            self.skip_whitespace();
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

        // Validate metrics terminal references if present
        if let Some(ref m_map) = metrics {
            for (metric_name, expr) in m_map {
                let mut term_refs = Vec::new();
                Self::collect_metric_terminals(expr, &mut term_refs);
                for term in term_refs {
                    if !terminals.contains(term) {
                        collector.report(ParseError::General {
                            span: span_to_source_span(&Span::new(start_pos, end_pos)),
                            message: format!(
                                "Metric '{}' references terminal '{}' which is not in device terminals: {:?}",
                                metric_name, term, terminals
                            ).into(),
                        });
                        return None;
                    }
                }
            }
        }

        // Validate spice block if present
        if let Some(ref spice) = spice_info {
            // Check that terminal_order references valid terminals
            for terminal in &spice.terminal_order {
                if !terminals.contains(terminal) {
                    collector.report(ParseError::General {
                        span: span_to_source_span(&Span::new(start_pos, end_pos)),
                        message: format!("SPICE terminal_order references '{}' which is not in device terminals: {:?}", terminal, terminals).into(),
                    });
                    return None;
                }
            }

            // Strict Contract Enforcement: Any requested SPICE parameter MUST be declared in metrics:
            if !spice.parameters.is_empty() {
                match metrics {
                    Some(ref m_map) => {
                        for param in &spice.parameters {
                            if !m_map.contains_key(param) {
                                collector.report(ParseError::General {
                                    span: span_to_source_span(&Span::new(start_pos, end_pos)),
                                    message: format!(
                                        "Device '{}' requests SPICE parameter '{}' but no corresponding extraction rule is declared in 'metrics:' block",
                                        name.as_str(), param
                                    ).into(),
                                });
                                return None;
                            }
                        }
                    }
                    None => {
                        collector.report(ParseError::General {
                            span: span_to_source_span(&Span::new(start_pos, end_pos)),
                            message: format!(
                                "Device '{}' requests SPICE parameters {:?} but is missing the REQUIRED 'metrics:' block",
                                name.as_str(), spice.parameters
                            ).into(),
                        });
                        return None;
                    }
                }
            }

            // Check that all terminals are in terminal_order (warning, not error)
            let terminal_set: FxHashSet<_> = terminals.iter().collect();
            let order_set: FxHashSet<_> = spice.terminal_order.iter().collect();

            if terminal_set != order_set {
                eprintln!(
                    "Warning: Device '{}' has terminals {:?} but SPICE terminal_order is {:?}",
                    name.as_str(),
                    terminals,
                    spice.terminal_order
                );
            }
        }

        Some(DeviceDefinition {
            name: name.clone(),
            is_exported,
            terminals: terminals.clone(),
            materials,
            tolerance,
            metrics,
            spice_info,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse SPICE export metadata block
    fn parse_spice_block(&mut self) -> Result<SpiceExportInfo, ParseError> {
        self.expect(&Token::Indent)?;

        let mut prefix: Option<char> = None;
        let mut terminal_order: Option<SmallVec<[CompactString; 4]>> = None;
        let mut parameters: Option<SmallVec<[CompactString; 4]>> = None;
        let mut model_name: Option<CompactString> = None;
        let mut parameter_style: Option<SpiceParameterStyle> = None;
        let mut subcircuit: Option<CompactString> = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Use expect_identifier_string to handle keywords-as-identifiers
            let field_name = match self.expect_identifier_string() {
                Ok(name) => name,
                Err(e) => {
                    return Err(e);
                }
            };

            match field_name.as_str() {
                "prefix" => {
                    self.expect(&Token::Colon)?;
                    self.skip_whitespace();

                    // Parse single character
                    if let Some(tok) = self.current() {
                        if let Token::Identifier(s) = &tok.token {
                            if s.len() == 1 {
                                prefix = Some(s.chars().next().unwrap());
                                self.advance();
                            } else {
                                return Err(self.error(
                                    "SPICE prefix must be a single character (R, C, L, M, D, etc.)",
                                ));
                            }
                        } else {
                            return Err(self.error("Expected SPICE prefix identifier"));
                        }
                    }
                }

                "terminal_order" => {
                    self.expect(&Token::Colon)?;
                    terminal_order = Some(self.parse_identifier_list()?);
                }

                "parameters" => {
                    self.expect(&Token::Colon)?;
                    parameters = Some(self.parse_identifier_list()?);
                }

                "model" => {
                    self.expect(&Token::Colon)?;
                    self.skip_whitespace();
                    model_name = Some(self.expect_identifier()?.name);
                }

                "parameter_style" => {
                    self.expect(&Token::Colon)?;
                    self.skip_whitespace();

                    let style_ident = self.expect_identifier()?;
                    parameter_style = Some(match style_ident.name.as_str() {
                        "positional" => SpiceParameterStyle::Positional,
                        "named" => SpiceParameterStyle::Named,
                        other => {
                            return Err(self.error(&format!(
                                "Unknown parameter_style: '{}'. Expected 'positional' or 'named'",
                                other
                            )));
                        }
                    });
                }

                "subcircuit" => {
                    self.expect(&Token::Colon)?;
                    self.skip_whitespace();
                    subcircuit = Some(self.expect_identifier()?.name);
                }

                _ => {
                    return Err(self.error(&format!(
                                "Unknown spice field: '{}'. Expected: prefix, terminal_order, parameters, parameter_style, model, or subcircuit",
                                field_name
                            )));
                }
            }

            self.skip_whitespace();
        }

        self.expect(&Token::Dedent)?;

        // STRICT VALIDATION - ALL required fields must be present
        // NO DEFAULTS, NO FALLBACKS - fail loudly if missing
        let prefix = prefix.ok_or_else(|| self.error("SPICE block missing REQUIRED field 'prefix'. Add 'prefix: <char>' (e.g., 'prefix: C' for capacitor)"))?;
        let terminal_order = terminal_order.ok_or_else(|| self.error("SPICE block missing REQUIRED field 'terminal_order'. Add 'terminal_order: [term1, term2, ...]'"))?;
        let parameter_style = parameter_style.ok_or_else(|| self.error("SPICE block missing REQUIRED field 'parameter_style'. Add 'parameter_style: positional' or 'parameter_style: named'"))?;

        // Parameters can be empty (for devices with no extracted parameters)
        let parameters = parameters.unwrap_or_default();

        Ok(SpiceExportInfo {
            prefix,
            terminal_order,
            parameters,
            model_name,
            parameter_style,
            subcircuit,
        })
    }

    /// Parse identifier list: [A, B, C]
    pub(crate) fn parse_identifier_list(
        &mut self,
    ) -> Result<SmallVec<[CompactString; 4]>, ParseError> {
        let mut result = SmallVec::new();

        self.expect(&Token::OpenBracket)?;
        self.skip_whitespace();

        if self.check(&Token::CloseBracket) {
            self.advance();
            return Ok(result);
        }

        loop {
            let ident = self.expect_identifier()?;
            result.push(ident.name);
            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect(&Token::CloseBracket)?;
        Ok(result)
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

    fn parse_metrics_mappings(&mut self) -> Result<FxHashMap<CompactString, MetricExpression>, ParseError> {
        let mut mappings = FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let metric_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;
            self.skip_whitespace();

            let expr = self.parse_single_metric_expression()?;
            self.skip_whitespace();

            if mappings.contains_key(metric_name.as_str()) {
                return Err(self.error(&format!(
                    "Duplicate metric specification for parameter '{}'",
                    metric_name
                )));
            }

            mappings.insert(metric_name.into(), expr);
        }

        if mappings.is_empty() {
            return Err(self.error("Metrics block cannot be empty"));
        }

        Ok(mappings)
    }

    fn parse_single_metric_expression(&mut self) -> Result<MetricExpression, ParseError> {
        self.parse_metric_expr()
    }

    fn parse_metric_expr(&mut self) -> Result<MetricExpression, ParseError> {
        let mut left = self.parse_metric_primary()?;
        self.skip_whitespace();

        while self.check(&Token::Slash) {
            self.advance(); // consume '/'
            self.skip_whitespace();
            let right = self.parse_metric_primary()?;
            self.skip_whitespace();
            left = MetricExpression::Divide(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_metric_primary(&mut self) -> Result<MetricExpression, ParseError> {
        self.skip_whitespace();

        if self.check(&Token::OpenParen) {
            self.advance();
            self.skip_whitespace();
            let expr = self.parse_metric_expr()?;
            self.skip_whitespace();
            self.expect(&Token::CloseParen)?;
            return Ok(expr);
        }

        let ident = self.expect_identifier_string()?;
        self.skip_whitespace();

        if !self.check(&Token::OpenParen) {
            // Identifier reference to another metric in the block (e.g. `SA`, `W`)
            return Ok(MetricExpression::Ref(ident.into()));
        }

        self.expect(&Token::OpenParen)?;
        self.skip_whitespace();

        match ident.as_str() {
            "span" => {
                let manifold = self.parse_manifold_expr()?;
                self.skip_whitespace();
                self.expect(&Token::Comma)?;
                self.skip_whitespace();

                let along_key = self.expect_identifier_string()?;
                if along_key != "along" {
                    return Err(self.error("Expected 'along:' argument in span()"));
                }
                self.expect(&Token::Colon)?;
                self.skip_whitespace();

                let mode = self.expect_identifier_string()?;
                self.skip_whitespace();
                self.expect(&Token::OpenParen)?;
                self.skip_whitespace();
                let from = self.expect_identifier_string()?;
                self.skip_whitespace();
                self.expect(&Token::Comma)?;
                self.skip_whitespace();
                let to = self.expect_identifier_string()?;
                self.skip_whitespace();
                self.expect(&Token::CloseParen)?;
                self.skip_whitespace();
                self.expect(&Token::CloseParen)?;

                match mode.as_str() {
                    "flux" => Ok(MetricExpression::SpanAlongFlux {
                        manifold,
                        from: from.into(),
                        to: to.into(),
                    }),
                    "transverse" => Ok(MetricExpression::SpanAlongTransverse {
                        manifold,
                        from: from.into(),
                        to: to.into(),
                    }),
                    other => Err(self.error(&format!(
                        "Unknown span direction '{}'. Expected 'flux(from, to)' or 'transverse(from, to)'",
                        other
                    ))),
                }
            }
            "area" => {
                let manifold = self.parse_manifold_expr()?;
                self.skip_whitespace();
                self.expect(&Token::CloseParen)?;
                Ok(MetricExpression::Area(manifold))
            }
            "perimeter" => {
                let manifold = self.parse_manifold_expr()?;
                self.skip_whitespace();
                self.expect(&Token::CloseParen)?;
                Ok(MetricExpression::Perimeter(manifold))
            }
            "resistance" => {
                let mut from = None;
                let mut to = None;
                let mut pos = Vec::new();

                while !self.check(&Token::CloseParen) && !self.is_at_end() {
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::CloseParen) {
                        break;
                    }

                    let arg_name = self.expect_identifier_string()?;
                    self.skip_whitespace();

                    if self.check(&Token::Colon) {
                        self.advance();
                        self.skip_whitespace();
                        let val = self.expect_identifier_string()?;
                        match arg_name.as_str() {
                            "from" => from = Some(val.into()),
                            "to" => to = Some(val.into()),
                            other => return Err(self.error(&format!("Unknown resistance argument: '{}'", other))),
                        }
                    } else {
                        pos.push(arg_name.into());
                    }

                    self.skip_whitespace();
                    if self.check(&Token::Comma) {
                        self.advance();
                        self.skip_whitespace();
                    } else if !self.check(&Token::CloseParen) {
                        return Err(self.error("Expected ',' or ')' in resistance arguments"));
                    }
                }
                self.expect(&Token::CloseParen)?;

                let from = from.or_else(|| pos.get(0).cloned()).ok_or_else(|| self.error("resistance requires 'from' argument"))?;
                let to = to.or_else(|| pos.get(1).cloned()).ok_or_else(|| self.error("resistance requires 'to' argument"))?;
                Ok(MetricExpression::Resistance { from, to })
            }
            "capacitance" => {
                let mut plate_a = None;
                let mut plate_b = None;
                let mut pos = Vec::new();

                while !self.check(&Token::CloseParen) && !self.is_at_end() {
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::CloseParen) {
                        break;
                    }

                    let arg_name = self.expect_identifier_string()?;
                    self.skip_whitespace();

                    if self.check(&Token::Colon) {
                        self.advance();
                        self.skip_whitespace();
                        let val = self.expect_identifier_string()?;
                        match arg_name.as_str() {
                            "plate_a" | "a" | "terminal_a" => plate_a = Some(val.into()),
                            "plate_b" | "b" | "terminal_b" => plate_b = Some(val.into()),
                            other => return Err(self.error(&format!("Unknown capacitance argument: '{}'", other))),
                        }
                    } else {
                        pos.push(arg_name.into());
                    }

                    self.skip_whitespace();
                    if self.check(&Token::Comma) {
                        self.advance();
                        self.skip_whitespace();
                    } else if !self.check(&Token::CloseParen) {
                        return Err(self.error("Expected ',' or ')' in capacitance arguments"));
                    }
                }
                self.expect(&Token::CloseParen)?;

                let plate_a = plate_a.or_else(|| pos.get(0).cloned()).ok_or_else(|| self.error("capacitance requires first plate argument"))?;
                let plate_b = plate_b.or_else(|| pos.get(1).cloned()).ok_or_else(|| self.error("capacitance requires second plate argument"))?;
                Ok(MetricExpression::Capacitance { plate_a, plate_b })
            }
            other => Err(self.error(&format!(
                "Unknown metric operator: '{}'. Expected: span, area, perimeter, resistance, capacitance, or metric identifier",
                other
            ))),
        }
    }

    /// Parse 2D manifold composition: `D - G`, `G & hull(S, D)`, `S | D`
    pub(crate) fn parse_manifold_expr(&mut self) -> Result<ManifoldExpr, ParseError> {
        let mut left = self.parse_manifold_primary()?;
        self.skip_whitespace();

        while self.check(&Token::Hyphen) || self.check(&Token::Ampersand) || self.check(&Token::Pipe) {
            let op = self.current().map(|t| t.token.clone()).unwrap();
            self.advance();
            self.skip_whitespace();
            let right = self.parse_manifold_primary()?;
            self.skip_whitespace();
            left = match op {
                Token::Hyphen => ManifoldExpr::Difference(Box::new(left), Box::new(right)),
                Token::Ampersand => ManifoldExpr::Intersect(Box::new(left), Box::new(right)),
                Token::Pipe => ManifoldExpr::Union(Box::new(left), Box::new(right)),
                _ => unreachable!(),
            };
        }

        Ok(left)
    }

    fn parse_manifold_primary(&mut self) -> Result<ManifoldExpr, ParseError> {
        self.skip_whitespace();
        if self.check(&Token::OpenParen) {
            self.advance();
            self.skip_whitespace();
            let expr = self.parse_manifold_expr()?;
            self.skip_whitespace();
            self.expect(&Token::CloseParen)?;
            return Ok(expr);
        }

        let name = self.expect_identifier_string()?;
        self.skip_whitespace();

        if name == "hull" && self.check(&Token::OpenParen) {
            self.advance(); // consume '('
            self.skip_whitespace();
            let a = self.parse_manifold_expr()?;
            self.skip_whitespace();
            self.expect(&Token::Comma)?;
            self.skip_whitespace();
            let b = self.parse_manifold_expr()?;
            self.skip_whitespace();
            self.expect(&Token::CloseParen)?;
            return Ok(ManifoldExpr::Hull(Box::new(a), Box::new(b)));
        }

        Ok(ManifoldExpr::Terminal(name.into()))
    }

    fn collect_metric_terminals<'b>(expr: &'b MetricExpression, out: &mut Vec<&'b CompactString>) {
        match expr {
            MetricExpression::Ref(_) => {}
            MetricExpression::SpanAlongFlux { manifold, from, to }
            | MetricExpression::SpanAlongTransverse { manifold, from, to } => {
                manifold.collect_terminals(out);
                out.push(from);
                out.push(to);
            }
            MetricExpression::Area(m) | MetricExpression::Perimeter(m) => {
                m.collect_terminals(out);
            }
            MetricExpression::Divide(a, b) => {
                Self::collect_metric_terminals(a, out);
                Self::collect_metric_terminals(b, out);
            }
            MetricExpression::Resistance { from, to } => {
                out.push(from);
                out.push(to);
            }
            MetricExpression::Capacitance { plate_a, plate_b } => {
                out.push(plate_a);
                out.push(plate_b);
            }
        }
    }
}
