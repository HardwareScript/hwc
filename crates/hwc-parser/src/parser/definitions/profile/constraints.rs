use super::super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};
use compact_str::CompactString;

impl super::super::super::Parser {
    /// Parse trace constraints block
    pub(super) fn parse_trace_constraints(&mut self) -> Result<TraceConstraints, ParseError> {
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
                    self.skip_whitespace();
                }
                "min_spacing" => {
                    min_spacing = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_width" => {
                    max_width = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_length" => {
                    max_length = Some(self.parse_measurement()?);
                    self.skip_whitespace();
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

    /// Parse layer constraints block
    pub(super) fn parse_layer_constraints(&mut self) -> Result<LayerConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut max_count = None;
        let mut min_thickness = None;
        let mut allowed_conductors = Vec::new();
        let mut allowed_dielectrics = Vec::new();

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
                    self.skip_whitespace();
                }
                "min_thickness" => {
                    min_thickness = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "allowed_conductors" => {
                    allowed_conductors = self.parse_identifier_array()?;
                    self.skip_whitespace();
                }
                "allowed_dielectrics" => {
                    allowed_dielectrics = self.parse_identifier_array()?;
                    self.skip_whitespace();
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
            allowed_conductors,
            allowed_dielectrics,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse array of identifiers: [Copper, Aluminum]
    pub(super) fn parse_identifier_array(&mut self) -> Result<Vec<CompactString>, ParseError> {
        self.expect(&Token::OpenBracket)?;
        let mut idents = Vec::new();

        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseBracket) {
                break;
            }

            let ident = self.expect_identifier()?;
            idents.push(ident.name.clone());

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            }
        }

        self.expect(&Token::CloseBracket)?;
        Ok(idents)
    }

    /// Parse clearance constraints block
    pub(super) fn parse_clearance_constraints(
        &mut self,
    ) -> Result<ClearanceConstraints, ParseError> {
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
                    self.skip_whitespace();
                }
                "safety_factor" => {
                    safety_factor = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "low_voltage_threshold" => {
                    low_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "medium_voltage_threshold" => {
                    medium_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "high_voltage_threshold" => {
                    high_voltage_threshold = Some(self.parse_measurement()?);
                    self.skip_whitespace();
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
    pub(super) fn parse_thermal_constraints(&mut self) -> Result<ThermalConstraints, ParseError> {
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
                    self.skip_whitespace();
                }
                "max_operating_temp" => {
                    max_operating_temp = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_temp_rise" => {
                    max_temp_rise = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "clustering_threshold" => {
                    clustering_threshold = Some(self.parse_measurement()?);
                    self.skip_whitespace();
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
    pub(super) fn parse_manufacturing_constraints(
        &mut self,
    ) -> Result<ManufacturingConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut copper_thickness = None;
        let mut ipc2221_k_external = None;
        let mut ipc2221_k_internal = None;
        let mut min_feature_size = None;
        let mut solder_mask_expansion = None;
        let mut solder_mask_thickness = None;
        // v0.1.7 ASIC Extensions
        let mut track_pitch = None;
        let mut grid_snapping = None;
        let mut dummy_fill = None;
        let mut dummy_fill_density = None;
        let mut dummy_fill_size = None;
        let mut dummy_fill_spacing = None;

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
                    self.skip_whitespace();
                }
                "ipc2221_k_external" => {
                    ipc2221_k_external = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "ipc2221_k_internal" => {
                    ipc2221_k_internal = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "min_feature_size" => {
                    min_feature_size = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "solder_mask_expansion" => {
                    solder_mask_expansion = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "solder_mask_thickness" => {
                    solder_mask_thickness = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                // v0.1.7 ASIC Extensions
                "track_pitch" => {
                    track_pitch = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "grid_snapping" => {
                    grid_snapping = Some(self.expect_boolean()?);
                    self.skip_whitespace();
                }
                "dummy_fill" => {
                    dummy_fill = Some(self.expect_boolean()?);
                    self.skip_whitespace();
                }
                "dummy_fill_density" => {
                    dummy_fill_density = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "dummy_fill_size" => {
                    dummy_fill_size = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "dummy_fill_spacing" => {
                    dummy_fill_spacing = Some(self.parse_measurement()?);
                    self.skip_whitespace();
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
            solder_mask_expansion,
            solder_mask_thickness,
            track_pitch,
            grid_snapping,
            dummy_fill,
            dummy_fill_density,
            dummy_fill_size,
            dummy_fill_spacing,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse export/visualization constraints block (v0.1.6: Anti-Aliasing Switch)
    pub(super) fn parse_export_constraints(&mut self) -> Result<ExportConstraints, ParseError> {
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
                    self.skip_whitespace();
                }
                "smoothing_tolerance" => {
                    smoothing_tolerance = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "corner_lock" => {
                    // Parse array of angles: [45, 90]
                    corner_lock = Some(self.parse_angle_array()?);
                    self.skip_whitespace();
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
    pub(super) fn parse_angle_array(&mut self) -> Result<Vec<u32>, ParseError> {
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

    /// Parse routing constraints block (v0.1.7)
    ///
    /// Syntax:
    /// ```hw
    /// routing:
    ///     m1: horizontal
    ///     m2: vertical
    ///     m3: horizontal
    ///     max_local_route_length: 10um
    ///     base_cost: 1
    ///     via_penalty: 50
    ///     direction_penalty: 10
    ///     tight_clearance_penalty: 2
    ///     crosstalk_penalty: 3
    ///     impedance_penalty: 1
    ///     reference_void_penalty: 5000000
    /// ```
    pub(super) fn parse_routing_constraints(&mut self) -> Result<RoutingConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut layer_directions = rustc_hash::FxHashMap::default();
        let mut max_local_route_length = None;
        let mut base_cost = None;
        let mut via_penalty = None;
        let mut direction_penalty = None;
        let mut tight_clearance_penalty = None;
        let mut crosstalk_penalty = None;
        let mut impedance_penalty = None;
        let mut reference_void_penalty = None;
        let mut net_priorities = rustc_hash::FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            // Check if this is a meta-field (not a layer direction)
            match field_name.as_str() {
                "max_local_route_length" => {
                    max_local_route_length = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                    continue;
                }
                "base_cost" => {
                    base_cost = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "via_penalty" => {
                    via_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "direction_penalty" => {
                    direction_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "tight_clearance_penalty" => {
                    tight_clearance_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "crosstalk_penalty" => {
                    crosstalk_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "impedance_penalty" => {
                    impedance_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "reference_void_penalty" => {
                    reference_void_penalty = Some(self.expect_integer()? as i64);
                    self.skip_whitespace();
                    continue;
                }
                "net_priorities" => {
                    // Marker — actual entries are net_priority_<name>: <level>
                    self.skip_whitespace();
                    continue;
                }
                _ if field_name.name.starts_with("net_priority_") => {
                    let net_name = &field_name.name["net_priority_".len()..];
                    let priority = self.expect_integer()? as u8;
                    net_priorities.insert(net_name.to_string(), priority);
                    self.skip_whitespace();
                    continue;
                }
                _ => {} // Fall through to layer direction parsing
            }

            let direction_str = self.expect_identifier()?;
            let direction = match direction_str.as_str() {
                "horizontal" => RoutingDirection::Horizontal,
                "vertical" => RoutingDirection::Vertical,
                "any" => RoutingDirection::Any,
                _ => {
                    return Err(self.error(&format!(
                        "Unknown routing direction: '{}' (expected 'horizontal', 'vertical', or 'any')",
                        direction_str
                    )));
                }
            };

            layer_directions.insert(field_name.to_string(), direction);
            self.skip_whitespace();
        }

        let end_pos = self.previous_span().end;

        Ok(RoutingConstraints {
            layer_directions,
            max_local_route_length,
            base_cost,
            via_penalty,
            direction_penalty,
            tight_clearance_penalty,
            crosstalk_penalty,
            impedance_penalty,
            reference_void_penalty,
            net_priorities,
            span: Span::new(start_pos, end_pos),
        })
    }
}
