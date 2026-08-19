//! Material definition parsing

use super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Material Definition Parsing
    // ========================================================================

    /// Parse material definition: `material Copper:` or `export material Copper:`
    ///
    /// Reports errors to collector and returns None if parsing fails.
    pub(in super::super) fn parse_material(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<MaterialDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Material) {
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

        let mut category = None;
        let mut process = None; // v0.1.7
        let mut symbol = None;
        let mut description = None;
        let mut properties = Vec::new();

        // eprintln!("[DEBUG] Starting material property parsing loop");
        let mut loop_iterations = 0;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations % 10 == 0 {
                // eprintln!("[DEBUG] Material parser loop iteration {}, token: {:?}",
                //     loop_iterations,
                //     self.current().map(|s| &s.token)
                // );
            }

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Material parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Material parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Check for property block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "category" => {
                            self.advance(); // consume 'category'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_material_category() {
                                Ok(cat) => category = Some(cat),
                                Err(e) => {
                                    collector.report(e);
                                    self.skip_whitespace();
                                    continue;
                                }
                            }
                            self.skip_whitespace();
                            continue;
                        }
                        "process" => {
                            self.advance(); // consume 'process'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_manufacturing_process() {
                                Ok(proc) => process = Some(proc),
                                Err(e) => {
                                    collector.report(e);
                                    self.skip_whitespace();
                                    continue;
                                }
                            }
                            self.skip_whitespace();
                            continue;
                        }
                        "properties" => {
                            self.advance(); // consume 'properties'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                continue;
                            }
                            // Skip any blank lines before indent
                            while self.check(&Token::Newline) {
                                self.advance();
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                continue;
                            }
                            match self.parse_properties() {
                                Ok(props) => properties = props,
                                Err(e) => {
                                    collector.report(e);
                                    // Try to recover by finding dedent
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

            // Handle other fields as identifiers
            let field_name = match self.expect_identifier() {
                Ok(n) => n,
                Err(e) => {
                    collector.report(e);
                    // Skip to next line to recover
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

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                // Skip to next line to recover
                while !self.is_at_end()
                    && !self.check(&Token::Newline)
                    && !self.check(&Token::Dedent)
                {
                    self.advance();
                }
                self.skip_whitespace();
                continue;
            }

            match field_name.as_str() {
                "gds_mapping" => {
                    // v0.2.3: Parse GDSII layer mapping: [layer: X, datatype: Y]
                    match self.parse_gds_mapping() {
                        Ok(mapping) => {
                            // Store in temporary variable, will be added to MaterialDefinition
                            // For now, we'll extract it after the loop
                            properties.push(Property {
                                key: "gds_mapping".into(),
                                value: PropertyValue::String(format!("{}:{}", mapping.0, mapping.1)),
                                span: self.previous_span(),
                            });
                        }
                        Err(e) => {
                            collector.report(e);
                        }
                    }
                    self.skip_whitespace();
                }
                "symbol" => {
                    // v0.2.1: Reject quoted string literals. The symbol must be a
                    // bare identifier (e.g. `symbol: Poly`, not `symbol: "Poly"`).
                    if self.check(&Token::String(Default::default())) {
                        collector.report(ParseError::General {
                            span: span_to_source_span(&self.current_span()),
                            message: "Material symbol must be a bare identifier (no quotes). \
                                      Example: symbol: Poly (not symbol: \"Poly\")"
                                .into(),
                        });
                        self.advance(); // consume the string token to recover
                        self.skip_whitespace();
                        continue;
                    }
                    match self.expect_identifier() {
                        Ok(id) => symbol = Some(id.name.to_string()),
                        Err(e) => collector.report(e),
                    }
                    self.skip_whitespace();
                }
                "description" => {
                    match self.expect_string() {
                        Ok(s) => description = Some(s),
                        Err(e) => collector.report(e),
                    }
                    self.skip_whitespace();
                }
                _ => {
                    collector
                        .report(self.error(&format!("Unknown material field: '{}'", field_name)));
                    // CRITICAL: Skip the entire line (including the value) to recover
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    self.skip_whitespace();
                }
            }

            // CRITICAL SAFETY: Ensure we made progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Material parser didn't advance, forcing progress");
                self.advance();
            }
        }

        // eprintln!("[DEBUG] Exited material property parsing loop after {} iterations", loop_iterations);

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        // v0.2.1: `process:` is now optional (Option<ManufacturingProcess>). When
        // omitted, `MaterialDefinition::get_process()` defaults to `Deposited`.
        // No mandatory-field validation is performed here.

        // Validate required fields
        let category = match category {
            Some(cat) => cat,
            None => {
                collector.report(ParseError::General {
                    span: span_to_source_span(&Span::new(start_pos, end_pos)),
                    message: "Material definition must have 'category' field".into(),
                });
                return None;
            }
        };

        // Extract visual properties from properties vector (v0.1.6 God-Tier Visual API)
        let mut color = None;
        let mut opacity = None;
        let mut outline_opacity = None;
        let mut roughness = None;
        let mut metallic = None;
        let mut ior = None;
        let mut clearcoat = None;
        let mut clearcoat_roughness = None;
        let mut subsurface = None;
        let mut anisotropy = None;
        let mut anisotropy_rotation = None;
        let mut texture = None;
        let mut gds_mapping = None; // v0.2.3

        for prop in &properties {
            match prop.key.as_str() {
                "color" => {
                    if let PropertyValue::String(s) = &prop.value {
                        color = Some(s.clone());
                    }
                }
                "opacity" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            opacity = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!("opacity must be between 0.0 and 1.0, got {}", n)
                                    .into(),
                            });
                        }
                    }
                }
                "outline_opacity" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            outline_opacity = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "outline_opacity must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "roughness" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            roughness = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "roughness must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "metallic" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            metallic = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!("metallic must be between 0.0 and 1.0, got {}", n)
                                    .into(),
                            });
                        }
                    }
                }
                "ior" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if n >= 1.0 {
                            ior = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!("ior must be >= 1.0, got {}", n).into(),
                            });
                        }
                    }
                }
                "clearcoat" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            clearcoat = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "clearcoat must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "clearcoat_roughness" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            clearcoat_roughness = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "clearcoat_roughness must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "subsurface" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            subsurface = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "subsurface must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "anisotropy" => {
                    if let PropertyValue::Number(n) = prop.value {
                        if (0.0..=1.0).contains(&n) {
                            anisotropy = Some(n);
                        } else {
                            collector.report(ParseError::General {
                                span: span_to_source_span(&prop.span),
                                message: format!(
                                    "anisotropy must be between 0.0 and 1.0, got {}",
                                    n
                                )
                                .into(),
                            });
                        }
                    }
                }
                "anisotropy_rotation" => {
                    if let PropertyValue::Number(n) = prop.value {
                        anisotropy_rotation = Some(n);
                    }
                }
                "texture" => {
                    if let PropertyValue::String(s) = &prop.value {
                        texture = Some(s.clone());
                    }
                }
                "gds_mapping" => {
                    // v0.2.3: Extract GDS mapping from stored string format "layer:datatype"
                    if let PropertyValue::String(s) = &prop.value {
                        if let Some((layer_str, datatype_str)) = s.split_once(':') {
                            if let (Ok(layer), Ok(datatype)) = (layer_str.parse::<u32>(), datatype_str.parse::<u32>()) {
                                gds_mapping = Some((layer, datatype));
                            }
                        }
                    }
                }
                _ => {} // Other properties are kept in the properties vector
            }
        }

        Some(MaterialDefinition {
            name,
            is_exported, // v0.2.0: Access control
            category,
            process, // v0.1.7
            symbol: symbol.map(|s: String| s.into()),
            description: description.map(|s: String| s.into()),
            properties,
            span: Span::new(start_pos, end_pos),
            color: color.map(|s: String| s.into()),
            opacity,
            outline_opacity,
            roughness,
            metallic,
            ior,
            clearcoat,
            clearcoat_roughness,
            subsurface,
            anisotropy,
            anisotropy_rotation,
            texture: texture.map(|s: String| s.into()),
            gds_mapping, // v0.2.3
        })
    }

    /// Parse manufacturing process keyword
    pub fn parse_manufacturing_process(&mut self) -> Result<ManufacturingProcess, ParseError> {
        let name = self.expect_identifier()?;

        match name.as_str() {
            "drilled_plated" => Ok(ManufacturingProcess::DrilledPlated),
            "deposited" => Ok(ManufacturingProcess::Deposited),
            "etched" => Ok(ManufacturingProcess::Etched),
            _ => Err(self.error(&format!("Unknown manufacturing process: '{}'", name))),
        }
    }

    /// Parse material alias: `material_alias M1: Copper` or `export material_alias M1: Copper`
    pub(in super::super) fn parse_material_alias(
        &mut self,
    ) -> Result<MaterialAliasDefinition, ParseError> {
        let start_pos = self.previous_span().start; // already advanced past "material_alias"

        // Note: export keyword already consumed by parse_definition dispatcher

        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        let target = self.expect_identifier()?;

        self.skip_whitespace();

        Ok(MaterialAliasDefinition {
            name,
            is_exported: false, // Will be set by caller if export was present
            target,
            span: Span::new(start_pos, self.previous_span().end),
        })
    }

    /// Parse material category
    ///
    /// Fundamental: conductor, insulator, semiconductor
    /// Bridge (Phase 1): ohmic_contact, die_interconnect, pcb_solder, barrier_layer, adhesive
    /// Zero-thickness (v0.2.1): mask
    fn parse_material_category(&mut self) -> Result<MaterialCategory, ParseError> {
        let ident = self.expect_identifier()?;
        match ident.name.to_lowercase().as_str() {
            // Fundamental categories
            "conductor" => Ok(MaterialCategory::Conductor),
            "insulator" => Ok(MaterialCategory::Insulator),
            "semiconductor" => Ok(MaterialCategory::Semiconductor),
            // Bridge categories (Phase 1 - BRIDGE-IMPLEMENTATION.md)
            "ohmic_contact" => Ok(MaterialCategory::OhmicContact),
            "die_interconnect" => Ok(MaterialCategory::DieInterconnect),
            "pcb_solder" => Ok(MaterialCategory::PcbSolder),
            "barrier_layer" => Ok(MaterialCategory::BarrierLayer),
            "adhesive" => Ok(MaterialCategory::Adhesive),
            // Zero-thickness fabrication instruction (v0.2.1)
            "mask" => Ok(MaterialCategory::Mask),
            _ => Err(self.error(&format!(
                "Invalid material category '{}'. Expected: conductor, insulator, semiconductor, \
                 ohmic_contact, die_interconnect, pcb_solder, barrier_layer, adhesive, or mask",
                ident
            ))),
        }
    }

    /// Parse properties block (key-value pairs)
    fn parse_properties(&mut self) -> Result<Vec<Property>, ParseError> {
        let mut properties = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Property names are now regular identifiers or keywords (soft keywords)
            // This allows keywords like 'material', 'profile', 'category' as property names
            let key = self.expect_identifier_or_keyword_string()?;
            self.expect(&Token::Colon)?;

            let value = self.parse_property_value()?;
            self.skip_whitespace();

            properties.push(Property {
                key: key.into(),
                value,
                span: self.previous_span(),
            });
        }

        Ok(properties)
    }

    /// Parse property value (measurement, string, number, or boolean)
    fn parse_property_value(&mut self) -> Result<PropertyValue, ParseError> {
        // Check for boolean values first (before sign handling)
        if self.check(&Token::True) {
            self.advance();
            return Ok(PropertyValue::Boolean(true));
        }
        if self.check(&Token::False) {
            self.advance();
            return Ok(PropertyValue::Boolean(false));
        }

        // Check for optional sign (v0.1.4: Plus/Hyphen are separate tokens)
        let sign = if self.check(&Token::Hyphen) {
            self.advance();
            -1.0
        } else if self.check(&Token::Plus) {
            self.advance();
            1.0
        } else {
            1.0
        };

        if let Some(current) = self.current() {
            match &current.token {
                Token::Measurement(_) => {
                    let mut m = self.parse_measurement()?;
                    m.value *= sign;
                    Ok(PropertyValue::Measurement(m))
                }
                Token::String(_) => {
                    let s = self.expect_string()?;
                    Ok(PropertyValue::String(s))
                }
                Token::Integer(_) | Token::Float(_) => {
                    let mut n = self.expect_number()?;
                    n *= sign;
                    Ok(PropertyValue::Number(n))
                }
                _ => {
                    Err(self
                        .error("Expected property value (measurement, string, number, or boolean)"))
                }
            }
        } else {
            Err(ParseError::UnexpectedEof {
                span: span_to_source_span(&self.previous_span()),
            })
        }
    }

    /// Parse GDSII layer mapping: `[layer: X, datatype: Y]`
    /// 
    /// v0.2.3: Parses the standardized GDSII layer/datatype tuple format used for
    /// 2D lithography export. This maps HardwareScript mask materials to their
    /// corresponding GDSII layer numbers in the foundry PDK.
    /// 
    /// # Syntax
    /// ```text
    /// gds_mapping: [layer: 64, datatype: 20]
    /// ```
    /// 
    /// # Returns
    /// `(layer, datatype)` tuple on success
    fn parse_gds_mapping(&mut self) -> Result<(u32, u32), ParseError> {
        // Expect opening bracket
        self.expect(&Token::OpenBracket)?;
        
        // Expect "layer" keyword
        let layer_keyword = self.expect_identifier()?;
        if layer_keyword.as_str() != "layer" {
            return Err(self.error("Expected 'layer' keyword in gds_mapping"));
        }
        
        self.expect(&Token::Colon)?;
        
        // Parse layer number
        let layer = match self.current() {
            Some(s) if matches!(s.token, Token::Integer(_)) => {
                let n = self.expect_number()? as u32;
                n
            }
            _ => return Err(self.error("Expected integer for layer number")),
        };
        
        // Expect comma
        self.expect(&Token::Comma)?;
        
        // Expect "datatype" keyword
        let datatype_keyword = self.expect_identifier()?;
        if datatype_keyword.as_str() != "datatype" {
            return Err(self.error("Expected 'datatype' keyword in gds_mapping"));
        }
        
        self.expect(&Token::Colon)?;
        
        // Parse datatype number
        let datatype = match self.current() {
            Some(s) if matches!(s.token, Token::Integer(_)) => {
                let n = self.expect_number()? as u32;
                n
            }
            _ => return Err(self.error("Expected integer for datatype number")),
        };
        
        // Expect closing bracket
        self.expect(&Token::CloseBracket)?;
        
        Ok((layer, datatype))
    }
}
