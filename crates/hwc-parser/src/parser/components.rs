//! Component placement and coordinate parsing

use super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};
use compact_str::CompactString;
use miette::SourceSpan;
use smallvec::SmallVec;

impl super::Parser {
    // ========================================================================
    // Component Placement Parsing
    // ========================================================================

    pub(super) fn parse_component_name(&mut self) -> Result<ComponentName, ParseError> {
        let name_span_start = self.current_span().start;
        let base_name = self.expect_identifier_string()?;

        // Check for array index: [i] or [0]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            let name_span_end = self.previous_span().end;

            Ok(crate::ast::ComponentName::indexed(
                base_name.into(),
                index_expr,
                crate::lexer::Span::new(name_span_start, name_span_end),
            ))
        } else {
            let name_span_end = self.previous_span().end;
            Ok(crate::ast::ComponentName::simple(
                base_name.into(),
                crate::lexer::Span::new(name_span_start, name_span_end),
            ))
        }
    }


    /// Parse component placement: `add Type (params) named Instance at [Z,X,Y] rotated angle`
    /// v0.1.6 Sprint 3.2: Supports array syntax: `add Type[count] named ArrayName`
    pub(super) fn parse_component_placement(&mut self) -> Result<ComponentPlacement, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Add)?;

        let component_type = self.expect_namespaced_identifier()?;

        // Parse optional array count: [count]
        let array_count = if self.check(&Token::OpenBracket) {
            self.advance();
            let count = if let Some(spanned) = self.current() {
                if let Token::Integer(n) = spanned.token {
                    if n <= 0 {
                        return Err(self.error("Array count must be positive"));
                    }
                    self.advance();
                    Some(n as usize)
                } else {
                    return Err(self.error("Expected integer array count"));
                }
            } else {
                return Err(self.error("Expected integer array count"));
            };
            self.expect(&Token::CloseBracket)?;
            count
        } else {
            None
        };

        // Parse optional parameters: (12V) or (4.7kΩ)
        let parameters = if self.check(&Token::OpenParen) {
            self.parse_parameters()?
        } else {
            SmallVec::new()
        };

        // Parse optional name: named Instance or named Adder[i]
        let name = if self.check(&Token::Named) {
            self.advance();
            Some(self.parse_component_name()?)
        } else {
            None
        };

        // Parse position: at [Z,X,Y] (Z is optional in v0.1.7 High-Level mode)
        self.expect(&Token::At)?;
        let position = self.parse_coordinate_optional_z()?;

        // v0.1.7: Support 'on layer: l1' or 'on z: 1mm' prepositional syntax
        let elevation = if self.check(&Token::On) {
            self.advance(); // consume 'on'
            if self.check_identifier("layer") {
                self.advance(); // consume 'layer'
                self.expect(&Token::Colon)?;
                let layer_name = self.expect_identifier()?;
                Some(Elevation::Semantic(layer_name))
            } else if self.check_identifier("z") {
                self.advance(); // consume 'z'
                self.expect(&Token::Colon)?;
                let start = self.parse_expression()?;
                
                let mut end = None;
                if self.check(&Token::To) {
                    self.advance(); // consume "to"
                    end = Some(self.parse_expression()?);
                }
                
                Some(Elevation::Physical { start, end })
            } else {
                return Err(self.error("Expected 'layer' or 'z' after 'on' keyword"));
            }
        } else {
            None
        };

        // Parse optional rotation: rotated 45 or rotated -30.5
        let rotation = if self.check(&Token::Rotated) {
            Some(self.parse_rotation()?)
        } else {
            None
        };

        // Parse optional configuration block (v0.1.6)
        // Can contain:
        // - Array config (Sprint 3.2): layout, pitch, merge
        // - Net bindings (Item #13): net: [pin: NetName, ...]
        let mut array_config = None;
        let mut pin_net_bindings = rustc_hash::FxHashMap::default();
        let mut waivers = Waivers::default();

        if self.check(&Token::Colon) {
            self.advance(); // consume colon
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            // Parse configuration block
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::Dedent) {
                    break;
                }

                if self.check_identifier("layout") || self.check_identifier("pitch") {
                    // Array configuration (layout/pitch)
                    if array_count.is_none() {
                        return Err(self.error("Array configuration (layout/pitch) requires array syntax: add Type[count]"));
                    }
                    if array_config.is_some() {
                        return Err(self.error("Duplicate array configuration"));
                    }
                    // Parse the entire array config block
                    array_config = Some(self.parse_array_config(array_count.unwrap(), &mut waivers)?);
                    break; // Array config consumes until dedent
                } else if self.check_identifier("merge") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    if self.check(&Token::True) {
                        self.advance();
                        waivers.merge = MergeWaiver::All;
                    } else if self.check(&Token::False) {
                        self.advance();
                        waivers.merge = MergeWaiver::None;
                    } else if self.check(&Token::OpenBracket) {
                        let terminals = self.parse_array_terminal_list()?;
                        waivers.merge = MergeWaiver::Specific(terminals);
                    } else {
                        return Err(self.error("Expected 'true', 'false', or '[list]' for merge property"));
                    }
                    self.skip_newlines();
                } else if self.check_identifier("floating") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.floating = self.parse_boolean()?;
                    self.skip_newlines();
                } else if self.check_identifier("isolated") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.isolated = self.parse_boolean()?;
                    self.skip_newlines();
                } else if self.check_identifier("snap_to_surface") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.snap_to_surface = self.parse_boolean()?;
                    self.skip_newlines();
                } else if self.check_identifier("virtual") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.virtual_component = self.parse_boolean()?;
                    self.skip_newlines();
                } else if self.check_identifier("locked") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.locked = self.parse_boolean()?;
                    self.skip_newlines();
                } else if self.check_identifier("net") {
                    // Net bindings (Item #13)
                    self.advance(); // consume 'net'
                    self.expect(&Token::Colon)?;
                    pin_net_bindings = self.parse_net_bindings()?;
                    self.skip_newlines();
                } else if self.check_identifier("allow_substrate_overlap") {
                    // LEGACY SUPPORT: Map to waivers.merge
                    self.advance();
                    self.expect(&Token::Colon)?;
                    if self.parse_boolean()? {
                        waivers.merge = MergeWaiver::All;
                    }
                    self.skip_newlines();
                } else {
                    return Err(self.error(
                        &format!("Unknown property in component configuration: '{}'", self.current().map(|t| t.token.to_string()).unwrap_or_default()),
                    ));
                }
            }

            self.expect(&Token::Dedent)?;
        } else if array_count.is_some() {
            // Array syntax without config block - use defaults
            array_config = Some(crate::ast::ArrayConfig {
                count: array_count.unwrap(),
                layout: crate::ast::ArrayLayout::HorizontalStack,
                pitch: crate::ast::Measurement {
                    value: 1.0,
                    unit: crate::ast::Unit::Millimeter,
                    span: Span::new(0, 0),
                },
                merge_terminals: SmallVec::new(),
                span: Span::new(start_pos, self.previous_span().end),
            });
        }

        self.skip_newlines();
        let end_pos = self.previous_span().end;

        Ok(ComponentPlacement {
            component_type,
            parameters,
            name,
            position,
            rotation,
            elevation,
            array_config,
            pin_net_bindings,
            waivers,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse array configuration block (indented)
    /// Expected to be called after consuming colon, newline, and indent tokens
    /// v0.1.6 Sprint 3.2: Parse array configuration with explicit merge intent
    fn parse_array_config(&mut self, count: usize, waivers: &mut Waivers) -> Result<crate::ast::ArrayConfig, ParseError> {
        let start_pos = self.current_span().start;

        let mut layout = crate::ast::ArrayLayout::HorizontalStack; // default
        let mut pitch: Option<crate::ast::Measurement> = None;
        let mut merge_terminals = SmallVec::new();

        // Parse indented block with layout, pitch, merge
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) {
                break;
            }

            if self.check_identifier("layout") {
                self.advance();
                self.expect(&Token::Colon)?;
                let layout_str = self.expect_identifier_string()?;
                layout = match layout_str.as_str() {
                    "horizontal_stack" => crate::ast::ArrayLayout::HorizontalStack,
                    "vertical_stack" => crate::ast::ArrayLayout::VerticalStack,
                    _ => {
                        return Err(self.error(&format!(
                        "Invalid array layout '{}'. Expected: horizontal_stack or vertical_stack",
                        layout_str
                    )))
                    }
                };
                self.skip_newlines();
            } else if self.check_identifier("pitch") {
                self.advance();
                self.expect(&Token::Colon)?;
                pitch = Some(self.parse_measurement()?);
                self.skip_newlines();
            } else if self.check_identifier("merge") {
                // EXPLICIT INTENT: User declares which terminals should be merged
                // Without this, overlapping geometry triggers P12: Geometric Collision
                self.advance();
                self.expect(&Token::Colon)?;
                if self.check(&Token::True) {
                    self.advance();
                    waivers.merge = MergeWaiver::All;
                } else if self.check(&Token::False) {
                    self.advance();
                    waivers.merge = MergeWaiver::None;
                } else if self.check(&Token::OpenBracket) {
                    merge_terminals = self.parse_array_terminal_list()?;
                    waivers.merge = MergeWaiver::Specific(merge_terminals.clone());
                } else {
                    return Err(self.error("Expected 'true', 'false', or '[list]' for merge property"));
                }
                self.skip_newlines();
            } else {
                return Err(
                    self.error("Expected 'layout', 'pitch', or 'merge' in array configuration")
                );
            }
        }

        // Pitch is required
        let pitch =
            pitch.ok_or_else(|| self.error("Array configuration requires 'pitch' field"))?;

        let end_pos = self.previous_span().end;

        Ok(crate::ast::ArrayConfig {
            count,
            layout,
            pitch,
            merge_terminals,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse terminal list for array configuration: [terminal1, terminal2, ...]
    pub(super) fn parse_array_terminal_list(&mut self) -> Result<SmallVec<[CompactString; 2]>, ParseError> {
        self.expect(&Token::OpenBracket)?;

        let mut terminals = SmallVec::new();

        // Parse first terminal
        if !self.check(&Token::CloseBracket) {
            terminals.push(self.expect_identifier_string()?.into());

            // Parse additional terminals separated by commas
            while self.check(&Token::Comma) {
                self.advance();
                terminals.push(self.expect_identifier_string()?.into());
            }
        }

        self.expect(&Token::CloseBracket)?;

        Ok(terminals)
    }

    /// Parse net bindings for component pins (v0.1.6 Item #13)
    /// Syntax: [pin1: NetName1, pin2: NetName2, ...]
    /// Supports:
    /// - Simple bindings: a: A[i]
    /// - Conditional bindings: carry_in: if i == 0 then CarryIn else Carry[i-1]
    fn parse_net_bindings(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, crate::ast::NetBinding>, ParseError> {
        self.expect(&Token::OpenBracket)?;

        let mut bindings = rustc_hash::FxHashMap::default();

        // Parse first binding
        if !self.check(&Token::CloseBracket) {
            let (pin, net) = self.parse_net_binding()?;
            bindings.insert(pin, net);

            // Parse additional bindings separated by commas
            while self.check(&Token::Comma) {
                self.advance();
                // Allow trailing comma
                if self.check(&Token::CloseBracket) {
                    break;
                }
                let (pin, net) = self.parse_net_binding()?;
                bindings.insert(pin, net);
            }
        }

        self.expect(&Token::CloseBracket)?;

        Ok(bindings)
    }

    /// Parse a single net binding: pin: NetName or pin: if condition then Net1 else Net2
    fn parse_net_binding(&mut self) -> Result<(CompactString, crate::ast::NetBinding), ParseError> {
        let pin_name = self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;

        // Check for conditional binding: if condition then Net1 else Net2
        if self.check(&Token::If) {
            self.advance(); // consume 'if'

            // Parse condition expression
            let condition = self.parse_expression()?;

            self.expect(&Token::Then)?;

            // Parse then net name (can be a simple identifier or indexed: A[i])
            let then_net = self.parse_net_name_string()?;

            self.expect(&Token::Else)?;

            // Parse else net name
            let else_net = self.parse_net_name_string()?;

            Ok((
                pin_name.into(),
                crate::ast::NetBinding::Conditional {
                    condition,
                    then_net: then_net.into(),
                    else_net: else_net.into(),
                },
            ))
        } else {
            // Simple binding: pin: NetName or pin: Net[i]
            let net_name = self.parse_net_name_string()?;
            Ok((
                pin_name.into(),
                crate::ast::NetBinding::Simple(net_name.into()),
            ))
        }
    }

    /// Parse a net name string with optional array indexing: NetName or Net[i] or Net[i-1]
    /// This is different from parse_net_name in helpers.rs which returns a NetName AST node
    fn parse_net_name_string(&mut self) -> Result<String, ParseError> {
        let base_name = self.expect_identifier_string()?;

        // Check for array syntax: Name[expr]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['

            // Parse the index expression as a string (we'll evaluate it later)
            let mut index_str = String::new();
            let mut bracket_depth = 1;

            while bracket_depth > 0 && !self.is_at_end() {
                if let Some(spanned_token) = self.current() {
                    match &spanned_token.token {
                        Token::OpenBracket => {
                            index_str.push('[');
                            bracket_depth += 1;
                            self.advance();
                        }
                        Token::CloseBracket => {
                            bracket_depth -= 1;
                            if bracket_depth > 0 {
                                index_str.push(']');
                            }
                            self.advance();
                        }
                        Token::Identifier(name) => {
                            index_str.push_str(name);
                            self.advance();
                        }
                        Token::Integer(n) => {
                            index_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Plus => {
                            index_str.push('+');
                            self.advance();
                        }
                        Token::Hyphen => {
                            index_str.push('-');
                            self.advance();
                        }
                        Token::Asterisk => {
                            index_str.push('*');
                            self.advance();
                        }
                        Token::Slash => {
                            index_str.push('/');
                            self.advance();
                        }
                        _ => {
                            return Err(self.error(&format!(
                                "Unexpected token in net name index: {}",
                                spanned_token.token
                            )));
                        }
                    }
                } else {
                    break;
                }
            }

            if bracket_depth != 0 {
                return Err(self.error("Unclosed bracket in net name index"));
            }

            Ok(format!("{}[{}]", base_name, index_str))
        } else {
            Ok(base_name)
        }
    }

    /// Check if current token is an identifier with specific value
    fn check_identifier(&self, expected: &str) -> bool {
        if let Some(spanned) = self.current() {
            if let Token::Identifier(s) = &spanned.token {
                return s == expected;
            }
        }
        false
    }

    /// Parse parameters: (resistance: 10kΩ, tolerance: 1%)
    /// v0.1.6: Only keyword arguments supported for self-documenting code
    pub(super) fn parse_parameters(
        &mut self,
    ) -> Result<SmallVec<[crate::ast::Parameter; 4]>, ParseError> {
        // eprintln!("[DEBUG parse_parameters] Starting, current token: {:?}", self.current().map(|t| &t.token));
        self.expect(&Token::OpenParen)?;

        let mut params = SmallVec::new();

        // Parse first parameter
        if !self.check(&Token::CloseParen) {
            // eprintln!("[DEBUG parse_parameters] Parsing first parameter");
            params.push(self.parse_parameter()?);
            // eprintln!("[DEBUG parse_parameters] First parameter parsed, current token: {:?}", self.current().map(|t| &t.token));

            // Parse additional parameters separated by commas
            while self.check(&Token::Comma) {
                // eprintln!("[DEBUG parse_parameters] Found comma, parsing next parameter");
                self.advance();
                params.push(self.parse_parameter()?);
                // eprintln!("[DEBUG parse_parameters] Parameter parsed, current token: {:?}", self.current().map(|t| &t.token));
            }
        }

        // eprintln!("[DEBUG parse_parameters] About to expect close paren, current token: {:?}", self.current().map(|t| &t.token));
        self.expect(&Token::CloseParen)?;
        // eprintln!("[DEBUG parse_parameters] Done, parsed {} parameters", params.len());

        Ok(params)
    }

    /// Parse a single parameter (keyword only - v0.1.6)
    /// Positional arguments are no longer supported for self-documenting code
    fn parse_parameter(&mut self) -> Result<crate::ast::Parameter, ParseError> {
        // v0.1.6: Only keyword arguments allowed
        // Syntax: name: value
        let name = self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;
        let value = self.parse_parameter_value()?;
        Ok(crate::ast::Parameter::Keyword {
            name: name.into(),
            value,
        })
    }

    /// Parse a parameter value: Measurement, String, or Number
    fn parse_parameter_value(&mut self) -> Result<crate::ast::ParameterValue, ParseError> {
        if let Some(spanned) = self.current() {
            match &spanned.token {
                // String literal: "Red"
                Token::String(s) => {
                    let val = s.clone();
                    self.advance();
                    Ok(crate::ast::ParameterValue::String(val))
                }
                // Measurement: 10kΩ, 5V, 100mm
                Token::Measurement(_) => {
                    let measurement = self.parse_measurement()?;
                    Ok(crate::ast::ParameterValue::Measurement(measurement))
                }
                // Plain number: 42, 3.14
                Token::Integer(n) => {
                    let val = *n as f64;
                    self.advance();
                    Ok(crate::ast::ParameterValue::Number(val))
                }
                Token::Float(f) => {
                    let val = *f;
                    self.advance();
                    Ok(crate::ast::ParameterValue::Number(val))
                }
                _ => Err(self.error("Expected parameter value (measurement, string, or number)")),
            }
        } else {
            Err(self.error("Expected parameter value"))
        }
    }

    // ========================================================================
    // Coordinate Parsing
    // ========================================================================

    /// Parse coordinate: [X,Y,Z] (positional) or [x:10, y:15, z:2] (declarative)
    pub(super) fn parse_coordinate(&mut self) -> Result<Coordinate, ParseError> {
        self.parse_coordinate_with_optional_z(false)
    }

    /// Parse coordinate with optional z (for mechanical features that span all layers)
    pub(super) fn parse_coordinate_optional_z(&mut self) -> Result<Coordinate, ParseError> {
        self.parse_coordinate_with_optional_z(true)
    }

    /// Parse coordinate with configurable z requirement
    ///
    /// Supports three syntaxes:
    /// 1. Positional: `[X, Y, Z]`
    /// 2. Declarative: `[x:10mm, y:15mm, z:1]`
    /// 3. Relative (v0.1.6): `AnchorName.edge + offset` or `last.edge + offset`
    fn parse_coordinate_with_optional_z(
        &mut self,
        z_optional: bool,
    ) -> Result<Coordinate, ParseError> {
        // Check for relative positioning syntax: AnchorName.edge + offset or last.edge + offset
        // This does NOT start with '[', so check before expecting bracket
        if let Some(token) = self.current() {
            // Check for 'last' keyword (special anchor for loop iterations)
            if matches!(token.token, Token::Last) {
                return self.parse_relative_coordinate();
            }

            if matches!(token.token, Token::Identifier(_)) {
                // Quick check: is the NEXT token a dot or open bracket?
                // This avoids expensive lookahead in the common case
                if let Some(next_token) = self.tokens.get(self.current + 1) {
                    match &next_token.token {
                        Token::Dot => {
                            // Simple case: Anchor.edge
                            return self.parse_relative_coordinate();
                        }
                        Token::OpenBracket => {
                            // Possible array syntax: Name[...].edge
                            // Only do expensive lookahead if we see a bracket
                            let mut lookahead_pos = self.current + 2; // Skip past identifier and '['
                            let mut bracket_depth = 1;

                            // Find the matching close bracket
                            while bracket_depth > 0 && lookahead_pos < self.tokens.len() {
                                if let Some(t) = self.tokens.get(lookahead_pos) {
                                    match t.token {
                                        Token::OpenBracket => bracket_depth += 1,
                                        Token::CloseBracket => bracket_depth -= 1,
                                        _ => {}
                                    }
                                }
                                lookahead_pos += 1;
                            }

                            // Now check if there's a dot after the closing bracket
                            if let Some(after_bracket) = self.tokens.get(lookahead_pos) {
                                if matches!(after_bracket.token, Token::Dot) {
                                    // This is relative positioning with array syntax
                                    return self.parse_relative_coordinate();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Not relative positioning, expect bracket for absolute coordinates
        self.expect(&Token::OpenBracket)?;

        // Check if this is declarative syntax by looking for identifier followed by colon
        let is_declarative = if let Some(token) = self.current() {
            matches!(token.token, Token::Identifier(_))
                && self
                    .tokens
                    .get(self.current + 1)
                    .is_some_and(|t| matches!(t.token, Token::Colon))
        } else {
            false
        };

        if is_declarative {
            self.parse_declarative_coordinate_impl(z_optional)
        } else {
            self.parse_positional_coordinate()
        }
    }

    /// Parse relative coordinate: AnchorName.edge + offset or last.edge + offset
    ///
    /// Syntax:
    /// - `M1.right + 1mm` - Single measurement offset
    /// - `M1.top + [0.5mm, 1mm, 0mm]` - Vector offset
    /// - `last.right + 1mm` - Reference to previous loop iteration (v0.1.6)
    ///
    /// Edges: left, right, top, bottom, front, back
    fn parse_relative_coordinate(&mut self) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;

        // Parse anchor name (may be 'last' keyword or identifier with optional array syntax)
        let anchor_name = if self.check(&Token::Last) {
            self.advance(); // consume 'last'
            "last".to_string()
        } else if self.check(&Token::Substrate) {
            self.advance(); // consume 'substrate'
            "substrate".to_string()
        } else {
            self.parse_anchor_name()?
        };
        let anchor_span = self.previous_span();

        // Expect dot
        self.expect(&Token::Dot)?;

        // Parse edge name
        let edge_str = self.expect_identifier_string()?;
        let edge = match edge_str.as_str() {
            "left" => Edge::Left,
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            "front" => Edge::Front,
            "back" => Edge::Back,
            "min_z" => Edge::MinZ,
            "max_z" => Edge::MaxZ,
            _ => {
                return Err(self.error(&format!(
                    "Invalid edge '{}'. Expected: left, right, top, bottom, front, back, min_z, or max_z",
                    edge_str
                )))
            }
        };

        // Optional: Expect '+' and offset
        let offset = if self.check(&Token::Plus) {
            self.advance(); // consume '+'

            // Parse offset: either single measurement or vector [x, y, z]
            if self.check(&Token::OpenBracket) {
                // Vector offset: [x, y, z]
                self.advance(); // consume '['

                let x = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let y = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let z = self.parse_expression()?;

                self.expect(&Token::CloseBracket)?;

                RelativeOffset::Vector { x, y, z }
            } else {
                // Single measurement offset
                let measurement = self.parse_measurement()?;
                RelativeOffset::Single(measurement)
            }
        } else {
            // Default to zero offset
            RelativeOffset::Single(crate::ast::Measurement {
                value: 0.0,
                unit: crate::ast::Unit::Millimeter,
                span: Span::new(self.previous_span().end, self.previous_span().end),
            })
        };

        let end_pos = self.previous_span().end;

        Ok(Coordinate::Relative(RelativePosition {
            anchor: AnchorReference {
                name: anchor_name.into(),
                span: anchor_span,
            },
            edge,
            offset,
            span: Span::new(start_pos, end_pos),
        }))
    }

    /// Parse anchor name with optional array syntax
    ///
    /// Supports:
    /// - Simple names: `M1`, `Resistor`
    /// - Array syntax: `Adder[0]`, `Component[i-1]`, `Block[i*2]`
    ///
    /// Returns the full anchor name as a string (e.g., "Adder[i-1]")
    pub(super) fn parse_anchor_name(&mut self) -> Result<String, ParseError> {
        let base_name = self.expect_identifier_string()?;

        // Check for array syntax: Name[expr]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['

            // Parse the index expression
            // We need to collect tokens until we find the matching ']'
            let mut index_str = String::new();
            let mut bracket_depth = 1;

            while bracket_depth > 0 && !self.is_at_end() {
                if let Some(spanned_token) = self.current() {
                    match &spanned_token.token {
                        Token::OpenBracket => {
                            index_str.push('[');
                            bracket_depth += 1;
                            self.advance();
                        }
                        Token::CloseBracket => {
                            bracket_depth -= 1;
                            if bracket_depth > 0 {
                                index_str.push(']');
                            }
                            self.advance();
                        }
                        Token::Identifier(name) => {
                            index_str.push_str(name);
                            self.advance();
                        }
                        Token::Integer(n) => {
                            index_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Plus => {
                            index_str.push('+');
                            self.advance();
                        }
                        Token::Hyphen => {
                            index_str.push('-');
                            self.advance();
                        }
                        Token::Asterisk => {
                            index_str.push('*');
                            self.advance();
                        }
                        Token::Slash => {
                            index_str.push('/');
                            self.advance();
                        }
                        _ => {
                            return Err(self.error(&format!(
                                "Unexpected token in array index: {}",
                                spanned_token.token
                            )));
                        }
                    }
                } else {
                    break;
                }
            }

            if bracket_depth != 0 {
                return Err(self.error("Unclosed bracket in array index"));
            }

            Ok(format!("{}[{}]", base_name, index_str))
        } else {
            Ok(base_name)
        }
    }

    /// Parse positional coordinate: [X, Y, Z]
    fn parse_positional_coordinate(&mut self) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;
        let x = self.parse_expression()?; // X first
        self.expect(&Token::Comma)?;
        let y = self.parse_expression()?; // Y second
        self.expect(&Token::Comma)?;
        let z = self.parse_expression()?; // Z third
        self.expect(&Token::CloseBracket)?;
        let end_pos = self.previous_span().end;

        Ok(Coordinate::Positional {
            x,
            y,
            z,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse declarative coordinate with optional z
    fn parse_declarative_coordinate_impl(
        &mut self,
        z_optional: bool,
    ) -> Result<Coordinate, ParseError> {
        let start_pos = self.current_span().start;
        let mut x = None;
        let mut y = None;
        let mut z = None;

        // Parse first coordinate pair
        self.parse_coordinate_pair(&mut x, &mut y, &mut z)?;

        // Parse remaining coordinate pairs
        while self.check(&Token::Comma) {
            self.advance(); // consume comma
            self.parse_coordinate_pair(&mut x, &mut y, &mut z)?;
        }

        self.expect(&Token::CloseBracket)?;
        let end_pos = self.previous_span().end;

        // Ensure required coordinates were specified
        let x = x.ok_or_else(|| self.error("Missing 'x' coordinate in declarative syntax"))?;
        let y = y.ok_or_else(|| self.error("Missing 'y' coordinate in declarative syntax"))?;

        let z = if z_optional {
            z.unwrap_or_else(|| Expression::Measurement {
                value: 0.0,
                unit: crate::ast::Unit::Millimeter,
                span: Span::new(start_pos, end_pos),
            })
        } else {
            z.ok_or_else(|| self.error("Missing 'z' coordinate in declarative syntax"))?
        };

        Ok(Coordinate::Declarative {
            x,
            y,
            z,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse a single coordinate pair: x:10, y:15, or z:2
    fn parse_coordinate_pair(
        &mut self,
        x: &mut Option<Expression>,
        y: &mut Option<Expression>,
        z: &mut Option<Expression>,
    ) -> Result<(), ParseError> {
        let axis = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        let value = self.parse_expression()?;

        match axis.as_str() {
            "x" => {
                if x.is_some() {
                    return Err(self.error("Duplicate 'x' coordinate"));
                }
                *x = Some(value);
            }
            "y" => {
                if y.is_some() {
                    return Err(self.error("Duplicate 'y' coordinate"));
                }
                *y = Some(value);
            }
            "z" => {
                if z.is_some() {
                    return Err(self.error("Duplicate 'z' coordinate"));
                }
                *z = Some(value);
            }
            _ => {
                return Err(self.error(&format!(
                    "Invalid coordinate axis '{}'. Expected 'x', 'y', or 'z' (lowercase only)",
                    axis
                )))
            }
        }

        Ok(())
    }

    /// Parse rotation: rotated 45 or rotated -30.5 or rotated 90° or rotated 90deg
    pub(super) fn parse_rotation(&mut self) -> Result<Rotation, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Rotated)?;

        // Check for optional negative sign (v0.1.4: Hyphen is separate token)
        let is_negative = if self.check(&Token::Hyphen) {
            self.advance();
            true
        } else {
            false
        };

        // v0.1.4: Can be a standalone number or a measurement with angle unit
        let angle = if let Some(current) = self.current() {
            match &current.token {
                Token::Measurement(m) => {
                    // Check if it's an angle measurement (now Custom)
                    if let crate::lexer::units::Unit::Custom(s) = &m.unit {
                        if s == "°" || s == "deg" {
                            let val = m.value;
                            self.advance();
                            val
                        } else {
                            return Err(self.error("Expected angle measurement after 'rotated'"));
                        }
                    } else {
                        return Err(self.error("Expected angle measurement after 'rotated'"));
                    }
                }
                Token::Integer(n) => {
                    let val = *n as f64;
                    self.advance();
                    val
                }
                Token::Float(f) => {
                    let val = *f;
                    self.advance();
                    val
                }
                _ => return Err(self.error("Expected number or angle measurement after 'rotated'")),
            }
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            return Err(ParseError::UnexpectedEof { span });
        };

        // Apply negative sign if present
        let final_angle = if is_negative { -angle } else { angle };

        let end_pos = self.previous_span().end;

        Ok(Rotation {
            angle: final_angle,
            span: Span::new(start_pos, end_pos),
        })
    }
}
