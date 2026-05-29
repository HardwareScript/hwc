//! Profile definition parsing (trace, via, layer, clearance constraints)

use super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Profile Definition Parsing
    // ========================================================================

    /// Parse profile definition: `define profile "HighVoltage":`
    pub(in super::super) fn parse_profile(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<ProfileDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Profile) {
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

        let mut description = None;
        let mut trace = None;
        let mut via = None;
        let mut layer = None;
        let mut clearance = None;
        let mut thermal = None;
        let mut manufacturing = None;
        let mut stackup = None;
        let mut export = None; // v0.1.6: Export & visualization rules
        let mut bridges = Vec::new(); // Phase 1: Bridge rules
        let mut other = rustc_hash::FxHashMap::default(); // v0.1.6: Custom fields

        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Profile parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Profile parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Phase 1: Bridge rules
            if self.check(&Token::Bridge) {
                match self.parse_bridge_rule() {
                    Ok(rule) => bridges.push(rule),
                    Err(e) => {
                        collector.report(e);
                        self.sync_to_next_definition();
                    }
                }
                continue;
            }

            // v0.1.6: Check for property block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "trace" => {
                            self.advance(); // consume 'trace'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            trace = self.parse_trace_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "via" => {
                            self.advance(); // consume 'via'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            via = self.parse_via_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "layer" => {
                            self.advance(); // consume 'layer'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            layer = self.parse_layer_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "clearance" => {
                            self.advance(); // consume 'clearance'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            clearance = self.parse_clearance_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            // Handle other fields as identifiers
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
                "description" => {
                    description = self.expect_string().ok();
                    self.skip_newlines();
                }
                "thermal" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    thermal = self.parse_thermal_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "manufacturing" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    manufacturing = self.parse_manufacturing_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "stackup" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    stackup = self.parse_stackup_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "export" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    export = self.parse_export_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                _ => {
                    // v0.1.6: Accept unknown fields and store in 'other' HashMap
                    // This allows custom tracking fields without compiler crashes

                    // Try to parse as string value (simple field.into())
                    if let Some(current) = self.current() {
                        if matches!(current.token, Token::String(_)) {
                            if let Ok(value) = self.expect_string() {
                                other.insert(field_name.name, value);
                            }
                            self.skip_newlines();
                            continue;
                        }
                    }

                    // Unknown constraint block - skip it
                    // This allows future extensions without breaking existing code
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }

                    // Skip the entire block
                    let mut depth = 1;
                    while depth > 0 && !self.is_at_end() {
                        if self.check(&Token::Indent) {
                            depth += 1;
                            self.advance();
                        } else if self.check(&Token::Dedent) {
                            depth -= 1;
                            self.advance();
                        } else {
                            self.advance();
                        }
                    }
                }
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Profile parser didn't advance, forcing progress");
                self.advance();
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(ProfileDefinition {
            name,
            description: description.map(|s: String| s.into()),
            trace,
            via,
            layer,
            clearance,
            thermal,
            manufacturing,
            stackup,
            export, // v0.1.6: Export & visualization rules
            bridges, // Phase 1: Bridge rules
            other,  // v0.1.6: Include custom fields
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse a bridge rule: `bridge Silicon to Copper: Cobalt_Silicide`
    /// or compound: `bridge Silicon to Copper: \n interface: ...`
    fn parse_bridge_rule(&mut self) -> Result<BridgeRule, ParseError> {
        let start_pos = self.current_span().start;
        
        self.expect(&Token::Bridge)?;
        
        let from_mat = self.expect_identifier()?;
        self.expect(&Token::To)?;
        let to_mat = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        
        // Two forms: 
        // 1. Single line: `bridge A to B: Material`
        // 2. Multi-line compound stack: `bridge A to B:\n  interface: ...`
        
        let mut interface_material = None;
        let mut interface_thickness = None;
        let mut fill_material = None;
        
        if self.check(&Token::Newline) {
            self.advance();
            self.expect(&Token::Indent)?;
            
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();
                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }
                
                let field_name = self.expect_identifier_or_keyword()?;
                self.expect(&Token::Colon)?;
                
                match field_name.as_str() {
                    "interface" => {
                        interface_material = Some(self.expect_identifier()?);
                        self.skip_newlines();
                    }
                    "thickness" => {
                        interface_thickness = Some(self.parse_measurement()?);
                        self.skip_newlines();
                    }
                    "fill" => {
                        fill_material = Some(self.expect_identifier()?);
                        self.skip_newlines();
                    }
                    _ => {
                        return Err(self.error(&format!("Unknown bridge constraint: '{}'", field_name)));
                    }
                }
            }
            self.expect(&Token::Dedent)?;
        } else {
            // Single line fallback
            interface_material = Some(self.expect_identifier()?);
            self.skip_newlines();
        }
        
        let end_pos = self.previous_span().end;
        
        let interface_material = interface_material.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Bridge rule must specify at least an 'interface' material".into(),
        })?;
        
        Ok(BridgeRule {
            from: from_mat.name,
            to: to_mat.name,
            interface_material: interface_material.name,
            interface_thickness,
            fill_material: fill_material.map(|id| id.name),
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse trace constraints block
    fn parse_trace_constraints(&mut self) -> Result<TraceConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut min_width = None;
        let mut min_spacing = None;
        let mut max_width = None;
        let mut max_length = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "min_width" => {
                    min_width = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "min_spacing" => {
                    min_spacing = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "max_width" => {
                    max_width = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "max_length" => {
                    max_length = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(self.error(&format!("Unknown trace constraint: '{}'", field_name)));
                }
            }
        }

        let end_pos = self.previous_span().end;

        // Validate required fields
        let min_width = min_width.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Trace constraints must have 'min_width' field".into(),
        })?;

        let min_spacing = min_spacing.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Trace constraints must have 'min_spacing' field".into(),
        })?;

        Ok(TraceConstraints {
            min_width,
            min_spacing,
            max_width,
            max_length,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse via constraints block
    fn parse_via_constraints(&mut self) -> Result<ViaConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut min_diameter = None;
        let mut min_annular_ring = None;
        let mut default_diameter = None;
        let mut min_spacing = None;
        let mut max_aspect_ratio = None;
        let mut default_via_fill = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "min_diameter" => {
                    min_diameter = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "min_annular_ring" => {
                    min_annular_ring = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "default_diameter" => {
                    default_diameter = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "min_spacing" => {
                    min_spacing = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "max_aspect_ratio" => {
                    max_aspect_ratio = Some(self.expect_number()?);
                    self.skip_newlines();
                }
                "default_via_fill" => {
                    default_via_fill = Some(self.expect_identifier()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(self.error(&format!("Unknown via constraint: '{}'", field_name)));
                }
            }
        }

        let end_pos = self.previous_span().end;

        // Validate required fields
        let min_diameter = min_diameter.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'min_diameter' field".into(),
        })?;

        let min_annular_ring = min_annular_ring.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'min_annular_ring' field".into(),
        })?;

        Ok(ViaConstraints {
            min_diameter,
            min_annular_ring,
            default_diameter,
            min_spacing,
            max_aspect_ratio,
            default_via_fill,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse layer constraints block
    fn parse_layer_constraints(&mut self) -> Result<LayerConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut max_count = None;
        let mut min_thickness = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "max_count" => {
                    max_count = Some(self.expect_integer()?);
                    self.skip_newlines();
                }
                "min_thickness" => {
                    min_thickness = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(self.error(&format!("Unknown layer constraint: '{}'", field_name)));
                }
            }
        }

        let end_pos = self.previous_span().end;

        Ok(LayerConstraints {
            max_count,
            min_thickness,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse clearance constraints block
    fn parse_clearance_constraints(&mut self) -> Result<ClearanceConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut high_voltage = None;
        let mut safety_factor = None;
        let mut low_voltage_threshold = None;
        let mut medium_voltage_threshold = None;
        let mut high_voltage_threshold = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "high_voltage" => {
                    high_voltage = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "safety_factor" => {
                    safety_factor = Some(self.expect_number()?);
                    self.skip_newlines();
                }
                "low_voltage_threshold" => {
                    low_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "medium_voltage_threshold" => {
                    medium_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "high_voltage_threshold" => {
                    high_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(
                        self.error(&format!("Unknown clearance constraint: '{}'", field_name))
                    );
                }
            }
        }

        let end_pos = self.previous_span().end;

        Ok(ClearanceConstraints {
            high_voltage,
            safety_factor,
            low_voltage_threshold,
            medium_voltage_threshold,
            high_voltage_threshold,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse thermal constraints block
    fn parse_thermal_constraints(&mut self) -> Result<ThermalConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut ambient_temp = None;
        let mut max_operating_temp = None;
        let mut max_temp_rise = None;
        let mut clustering_threshold = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "ambient_temp" => {
                    ambient_temp = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "max_operating_temp" => {
                    max_operating_temp = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "max_temp_rise" => {
                    max_temp_rise = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "clustering_threshold" => {
                    clustering_threshold = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(
                        self.error(&format!("Unknown thermal constraint: '{}'", field_name))
                    );
                }
            }
        }

        let end_pos = self.previous_span().end;

        // Validate required fields
        let ambient_temp = ambient_temp.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Thermal constraints must have 'ambient_temp' field".into(),
        })?;

        let max_operating_temp = max_operating_temp.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Thermal constraints must have 'max_operating_temp' field".into(),
        })?;

        let max_temp_rise = max_temp_rise.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Thermal constraints must have 'max_temp_rise' field".into(),
        })?;

        Ok(ThermalConstraints {
            ambient_temp,
            max_operating_temp,
            max_temp_rise,
            clustering_threshold,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse manufacturing constraints block
    fn parse_manufacturing_constraints(&mut self) -> Result<ManufacturingConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut copper_thickness = None;
        let mut ipc2221_k_external = None;
        let mut ipc2221_k_internal = None;
        let mut min_feature_size = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "copper_thickness" => {
                    copper_thickness = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "ipc2221_k_external" => {
                    ipc2221_k_external = Some(self.expect_number()?);
                    self.skip_newlines();
                }
                "ipc2221_k_internal" => {
                    ipc2221_k_internal = Some(self.expect_number()?);
                    self.skip_newlines();
                }
                "min_feature_size" => {
                    min_feature_size = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(self.error(&format!(
                        "Unknown manufacturing constraint: '{}'",
                        field_name
                    )));
                }
            }
        }

        let end_pos = self.previous_span().end;

        Ok(ManufacturingConstraints {
            copper_thickness,
            ipc2221_k_external,
            ipc2221_k_internal,
            min_feature_size,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse physical layer stackup block (v0.1.7 Z-Axis Abstraction)
    ///
    /// Syntax:
    ///     stackup:
    ///         l1: [material: Copper, thickness: 35um]
    ///         d1: [material: FR4,    thickness: 0.2mm]
    ///         ...
    fn parse_stackup_constraints(&mut self) -> Result<LayerStackup, ParseError> {
        let _start_pos = self.current_span().start;
        let mut layers = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Layer name: l1, d1, inner1, etc.
            let name = self.expect_identifier()?;

            self.expect(&Token::Colon)?;

            // Expect opening bracket for the layer properties
            self.expect(&Token::OpenBracket)?;

            let mut material = None;
            let mut thickness = None;

            // Parse key: value pairs inside the brackets (comma separated)
            while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::CloseBracket) {
                    break;
                }

                // "material" is a soft keyword elsewhere; allow it as a stackup property key
                let key = self.expect_identifier_or_keyword_string()?;
                self.expect(&Token::Colon)?;

                match key.as_str() {
                    "material" => {
                        let mat = self.expect_namespaced_identifier_string()?;
                        material = Some(mat);
                    }
                    "thickness" => {
                        // Use parse_expression so we can support variables/expressions later
                        thickness = Some(self.parse_expression()?);
                    }
                    _ => {
                        return Err(
                            self.error(&format!("Unknown stackup layer property: '{}'", key))
                        );
                    }
                }

                // Optional comma between properties
                if self.check(&Token::Comma) {
                    self.advance();
                }

                self.skip_whitespace();
            }

            self.expect(&Token::CloseBracket)?;
            self.skip_newlines();

            let material = material.ok_or_else(|| {
                self.error("Stackup layer definition must include 'material'")
            })?;
            let thickness = thickness.ok_or_else(|| {
                self.error("Stackup layer definition must include 'thickness'")
            })?;

            layers.push(StackupLayer {
                name,
                material: material.into(),
                thickness,
            });
        }

        let _end_pos = self.previous_span().end;

        Ok(LayerStackup { layers })
    }

    /// Parse export/visualization constraints block (v0.1.6: Anti-Aliasing Switch)
    fn parse_export_constraints(&mut self) -> Result<ExportConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut antialiasing = false; // Conservative default
        let mut smoothing_tolerance = None;
        let mut corner_lock = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "antialiasing" => {
                    antialiasing = self.expect_boolean()?;
                    self.skip_newlines();
                }
                "smoothing_tolerance" => {
                    smoothing_tolerance = Some(self.parse_measurement()?);
                    self.skip_newlines();
                }
                "corner_lock" => {
                    // Parse array of angles: [45, 90]
                    corner_lock = Some(self.parse_angle_array()?);
                    self.skip_newlines();
                }
                _ => {
                    return Err(self.error(&format!("Unknown export constraint: '{}'", field_name)));
                }
            }
        }

        let end_pos = self.previous_span().end;

        Ok(ExportConstraints {
            antialiasing,
            smoothing_tolerance,
            corner_lock,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse array of angles for corner_lock: [45, 90]
    fn parse_angle_array(&mut self) -> Result<Vec<u32>, ParseError> {
        self.expect(&Token::OpenBracket)?;
        let mut angles = Vec::new();

        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseBracket) {
                break;
            }

            let angle = self.expect_number()? as u32;
            angles.push(angle);

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            }
        }

        self.expect(&Token::CloseBracket)?;
        Ok(angles)
    }
}
