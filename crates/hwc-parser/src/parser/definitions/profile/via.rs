use super::super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};
use rustc_hash::FxHashMap;

impl super::super::super::Parser {
    /// Parse via constraints block
    pub(super) fn parse_via_constraints(&mut self) -> Result<ViaConstraints, ParseError> {
        let start_pos = self.current_span().start;
        let mut min_diameter = None;
        let mut min_enclosure = None;
        let mut default_diameter = None;
        let mut min_spacing = None;
        let mut max_aspect_ratio = None;
        let mut default_via_fill = None;
        let mut shape = None;
        let mut contact_depth = None;
        let mut material_contact_depths = None;
        let mut min_contact_depth = None;
        let mut max_contact_depth = None;
        // v0.1.7 ASIC Extensions
        let mut enclosures = None;
        let mut allow_stacked_vias = None;
        let mut min_stagger_offset = None;

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
                    self.skip_whitespace();
                }
                "min_enclosure" => {
                    min_enclosure = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "default_diameter" => {
                    default_diameter = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "min_spacing" => {
                    min_spacing = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_aspect_ratio" => {
                    max_aspect_ratio = Some(self.expect_number()?);
                    self.skip_whitespace();
                }
                "default_via_fill" => {
                    default_via_fill = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                "shape" => {
                    shape = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                "contact_depth" => {
                    // v0.2.1: Parse as expression (supports percentages, measurements, arithmetic)
                    contact_depth = Some(self.parse_expression()?);
                    self.skip_whitespace();
                }
                "material_contact_depths" => {
                    // v0.2.1: Parse material-specific depth map
                    material_contact_depths = Some(self.parse_material_depth_map()?);
                    self.skip_whitespace();
                }
                "min_contact_depth" => {
                    min_contact_depth = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "max_contact_depth" => {
                    max_contact_depth = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                // v0.1.7 ASIC Extensions
                "enclosures" => {
                    // v0.2.2: Support both bracket and indented block syntax
                    // Bracket: enclosures: [m1: 30nm, m2: 40nm]
                    // Indented: enclosures:
                    //               capm: 500nm
                    //               metal4: 50nm
                    if self.check(&Token::OpenBracket) {
                        enclosures = Some(self.parse_enclosure_map_bracket()?);
                    } else {
                        enclosures = Some(self.parse_enclosure_map_indented()?);
                    }
                    self.skip_whitespace();
                }
                "allow_stacked_vias" => {
                    allow_stacked_vias = Some(self.expect_boolean()?);
                    self.skip_whitespace();
                }
                "min_stagger_offset" => {
                    min_stagger_offset = Some(self.parse_measurement()?);
                    self.skip_whitespace();
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

        let min_enclosure = min_enclosure.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'min_enclosure' field".into(),
        })?;

        let contact_depth = contact_depth.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Via constraints must have 'contact_depth' field. Specify depth as percentage (50%), absolute measurement (150nm), or expression.\n\nExamples:\n  contact_depth: 50%\n  contact_depth: 150nm\n  contact_depth: 0%  (surface contact)\n  contact_depth: 100%  (complete penetration)".into(),
        })?;

        Ok(ViaConstraints {
            min_diameter,
            min_enclosure,
            default_diameter,
            min_spacing,
            max_aspect_ratio,
            default_via_fill,
            shape,
            contact_depth,
            material_contact_depths,
            min_contact_depth,
            max_contact_depth,
            enclosures,
            allow_stacked_vias,
            min_stagger_offset,
            span: Span::new(start_pos, end_pos),
        })
    }

    fn parse_material_depth_map(&mut self) -> Result<FxHashMap<String, Expression>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut map = FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            let material_name = self.expect_identifier()?.to_string();
            self.expect(&Token::Colon)?;
            self.skip_whitespace();
            let depth_expr = self.parse_expression()?;
            map.insert(material_name, depth_expr);

            self.skip_whitespace();
        }

        self.expect(&Token::Dedent)?;
        Ok(map)
    }

    /// Parse enclosure map with bracket syntax: [m1: 30nm, m2: 40nm, m3: 50nm]
    fn parse_enclosure_map_bracket(&mut self) -> Result<FxHashMap<String, Measurement>, ParseError> {
        self.expect(&Token::OpenBracket)?;
        let mut map = FxHashMap::default();

        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseBracket) {
                break;
            }

            let layer_name = self.expect_identifier()?.to_string();
            self.expect(&Token::Colon)?;
            let measurement = self.parse_measurement()?;
            map.insert(layer_name, measurement);

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            }
        }

        self.expect(&Token::CloseBracket)?;
        Ok(map)
    }

    /// Parse enclosure map with indented block syntax (v0.2.2):
    /// enclosures:
    ///     capm: 500nm
    ///     metal4: 50nm
    fn parse_enclosure_map_indented(&mut self) -> Result<FxHashMap<String, Measurement>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut map = FxHashMap::default();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            let layer_name = self.expect_identifier()?.to_string();
            self.expect(&Token::Colon)?;
            self.skip_whitespace();
            let measurement = self.parse_measurement()?;
            map.insert(layer_name, measurement);

            self.skip_whitespace();
        }

        self.expect(&Token::Dedent)?;
        Ok(map)
    }

    /// Parse explicit via definition: `via Name: ...` (v0.1.7)
    pub(super) fn parse_via_definition(&mut self) -> Result<ViaDefinition, ParseError> {
        let start_pos = self.current_span().start;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut diameter = None;
        let mut enclosure = None;
        let mut from_layer = None;
        let mut to_layer = None;
        let mut material = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "diameter" => {
                    diameter = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "enclosure" => {
                    enclosure = Some(self.parse_measurement()?);
                    self.skip_whitespace();
                }
                "spanning" => {
                    // Syntax: `spanning: layer: inner2 to inner1`
                    // or just `spanning: inner2 to inner1`
                    if self.check_identifier("layer") {
                        self.advance();
                        self.expect(&Token::Colon)?;
                    }
                    from_layer = Some(self.expect_identifier()?);
                    self.expect(&Token::To)?;
                    to_layer = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                "material" => {
                    material = Some(self.expect_identifier()?);
                    self.skip_whitespace();
                }
                _ => {
                    return Err(
                        self.error(&format!("Unknown via definition field: '{}'", field_name))
                    );
                }
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        let diameter =
            diameter.ok_or_else(|| self.error("Via definition must include 'diameter'"))?;
        let enclosure =
            enclosure.ok_or_else(|| self.error("Via definition must include 'enclosure'"))?;
        let from_layer =
            from_layer.ok_or_else(|| self.error("Via definition must include 'spanning'"))?;
        let to_layer =
            to_layer.ok_or_else(|| self.error("Via definition must include 'spanning'"))?;

        Ok(ViaDefinition {
            name,
            diameter,
            enclosure,
            from_layer,
            to_layer,
            material,
            span: Span::new(start_pos, end_pos),
        })
    }
}
