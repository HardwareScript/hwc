//! Component, pour, polygon, and contact placement parsing

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse pour placement: `add pour(Copper) named GND_Plane on z:2:`
    /// Supports namespaced materials: `add pour(Metals.Copper) named Trace1 on z:1:`
    pub(in crate::parser) fn parse_pour(&mut self) -> Result<PourPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Pour)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.expect(&Token::On)?;

        // v0.1.7 Z-Axis Abstraction: support both physical (z:) and semantic (layer:)
        let elevation = if self.check(&Token::Identifier("layer".into())) {
            self.advance(); // consume "layer"
            self.expect(&Token::Colon)?;
            let layer_name = self.expect_identifier()?;
            if layer_name.as_str() == "self" {
                Elevation::Relative
            } else {
                Elevation::Semantic(layer_name)
            }
        } else {
            // Physical: `on z: <expr>` or `on z: <expr> to <expr>`
            let coord_name = self.expect_identifier()?;
            if coord_name.as_str() != "z" {
                return Err(self.error("Expected 'z' or 'layer' for pour elevation"));
            }
            self.expect(&Token::Colon)?;
            
            if self.check(&Token::Identifier("relative".into())) {
                self.advance();
                Elevation::Relative
            } else {
                let start = self.parse_expression()?;
                
                let mut end = None;
                if self.check(&Token::To) {
                    self.advance(); // consume "to"
                    end = Some(self.parse_expression()?);
                }
                
                Elevation::Physical { start, end }
            }
        };

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut boundary = None;
        let mut net = None;
        let mut device = None;
        let mut thermal_relief = false;
        let mut waivers = Waivers::default();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // v0.1.6: Property names can be keywords (soft keywords)
            // This allows 'device' keyword to be used as property name: device: M1.gate
            let field_name = self.expect_identifier_or_keyword_string()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "boundary" => {
                    let from = self.parse_coordinate_optional_z()?;
                    self.expect(&Token::To)?;
                    let to = self.parse_coordinate_optional_z()?;
                    boundary = Some((from, to));
                }
                "net" => {
                    net = Some(self.parse_net_name()?);
                }
                "device" => {
                    // Phase 4: Parse device binding (DeviceName.terminal.into())
                    device = Some(self.parse_device_binding()?);
                }
                "thermal_relief" => {
                    let val = self.expect_identifier()?;
                    thermal_relief = val.as_str() == "true";
                }
                "merge" => {
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
                }
                "floating" => {
                    let val = self.expect_identifier()?;
                    waivers.floating = val.as_str() == "true";
                }
                "isolated" => {
                    let val = self.expect_identifier()?;
                    waivers.isolated = val.as_str() == "true";
                }
                "snap_to_surface" => {
                    let val = self.expect_identifier()?;
                    waivers.snap_to_surface = val.as_str() == "true";
                }
                "virtual" => {
                    let val = self.expect_identifier()?;
                    waivers.virtual_component = val.as_str() == "true";
                }
                "locked" => {
                    let val = self.expect_identifier()?;
                    waivers.locked = val.as_str() == "true";
                }
                _ => {
                    return Err(self.error(&format!("Unknown pour property: '{}'", field_name)));
                }
            }

            self.expect(&Token::Newline)?;
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(PourPlacement {
            material: material.into(),
            name: name.into(),
            elevation,
            boundary,
            net,
            device,
            thermal_relief,
            waivers,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse polygon placement: `add polygon(Copper) named WiFi_Antenna at [x:10, y:10, z:1]:`
    pub(in crate::parser) fn parse_polygon(&mut self) -> Result<PolygonPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Polygon)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.expect(&Token::At)?;
        let position = self.parse_coordinate()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut points = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            if field_name.as_str() == "points" {
                self.expect(&Token::Newline)?;
                self.expect(&Token::Indent)?;

                while !self.is_at_end() && !self.check(&Token::Dedent) {
                    if self.check(&Token::Newline) {
                        self.advance();
                        continue;
                    }

                    // Parse point: - [x, y] or - [xmm, ymm]
                    self.expect(&Token::Hyphen)?;
                    self.expect(&Token::OpenBracket)?;

                    let x = if let Some(current) = self.current() {
                        let val = match &current.token {
                            Token::Integer(n) => *n as f64,
                            Token::Float(f) => *f,
                            Token::Measurement(m) => m.value,
                            _ => {
                                return Err(
                                    self.error("Expected number or measurement for x coordinate")
                                )
                            }
                        };
                        self.advance();
                        val
                    } else {
                        return Err(self.error("Expected x coordinate"));
                    };

                    self.expect(&Token::Comma)?;

                    let y = if let Some(current) = self.current() {
                        let val = match &current.token {
                            Token::Integer(n) => *n as f64,
                            Token::Float(f) => *f,
                            Token::Measurement(m) => m.value,
                            _ => {
                                return Err(
                                    self.error("Expected number or measurement for y coordinate")
                                )
                            }
                        };
                        self.advance();
                        val
                    } else {
                        return Err(self.error("Expected y coordinate"));
                    };

                    self.expect(&Token::CloseBracket)?;
                    points.push((x, y));

                    self.expect(&Token::Newline)?;
                }

                self.expect(&Token::Dedent)?;
            } else {
                return Err(self.error(&format!("Unknown polygon property: '{}'", field_name)));
            }

            if !self.check(&Token::Dedent) {
                self.expect(&Token::Newline)?;
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(PolygonPlacement {
            material: material.into(),
            name: name.into(),
            position,
            points: points.into(),
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse contact/via placement: `add contact(Tungsten) at [x:500um, y:325um] spanning z:6 to z:8`
    ///
    /// Syntax:
    /// - `add contact(Material) at [x:X, y:Y] spanning z:FROM to z:TO`
    /// - Optional: `named Name` for identification
    /// - Optional: `net: NetName` to connect to a net
    /// - Optional: `diameter: Xmm` to specify via diameter
    pub(in crate::parser) fn parse_contact(&mut self) -> Result<ContactPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Contact)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        // Optional: named Name
        let name = if self.check(&Token::Named) {
            self.advance();
            Some(self.parse_component_name()?)
        } else {
            None
        };

        // Optional: net: NetName
        let net = if self.check(&Token::Identifier("net".into())) {
            self.advance(); // consume 'net'
            self.expect(&Token::Colon)?;
            Some(self.parse_net_name()?)
        } else {
            None
        };

        self.expect(&Token::At)?;
        let position = self.parse_coordinate_optional_z()?;

        self.expect(&Token::Spanning)?;

        // v0.1.7 Z-Axis Abstraction: support both `spanning z: A to z: B` and `spanning layer: l1 to l2`
        let (from_elevation, to_elevation) = if self.check(&Token::Identifier("layer".into())) {
            self.advance(); // consume "layer"
            self.expect(&Token::Colon)?;
            let from_name = self.expect_identifier()?;
            let from_elev = if from_name.as_str() == "self" { Elevation::Relative } else { Elevation::Semantic(from_name) };
            
            self.expect(&Token::To)?;
            
            // Consume optional second "layer" keyword
            if self.check(&Token::Identifier("layer".into())) {
                self.advance();
                self.expect(&Token::Colon)?;
            }
            let to_name = self.expect_identifier()?;
            let to_elev = if to_name.as_str() == "self" { Elevation::Relative } else { Elevation::Semantic(to_name) };
            
            (from_elev, to_elev)
        } else {
            // Physical / legacy
            let from_coord = self.expect_identifier()?;
            if from_coord.as_str() != "z" {
                return Err(self.error("Expected 'z' or 'layer' for contact elevation"));
            }
            self.expect(&Token::Colon)?;
            
            let from_elev = if self.check(&Token::Identifier("relative".into())) {
                self.advance();
                Elevation::Relative
            } else {
                Elevation::Physical { start: self.parse_expression()?, end: None }
            };

            self.expect(&Token::To)?;

            let to_coord = self.expect_identifier()?;
            if to_coord.as_str() != "z" {
                return Err(self.error("Expected 'z' or 'layer' for contact elevation"));
            }
            self.expect(&Token::Colon)?;
            
            let to_elev = if self.check(&Token::Identifier("relative".into())) {
                self.advance();
                Elevation::Relative
            } else {
                Elevation::Physical { start: self.parse_expression()?, end: None }
            };

            (from_elev, to_elev)
        };

        // Optional: properties block
        let (properties, net_in_block) = if self.check(&Token::Colon) {
            self.advance();
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            let mut props = rustc_hash::FxHashMap::default();
            let mut net_in_block = None;

            while !self.is_at_end() && !self.check(&Token::Dedent) {
                if self.check(&Token::Newline) {
                    self.advance();
                    continue;
                }

                let field_name = self.expect_identifier_or_keyword_string()?;
                self.expect(&Token::Colon)?;

                if field_name == "net" {
                    net_in_block = Some(self.parse_net_name()?);
                } else {
                    // v0.1.9: Generic property parsing
                    let expr = if self.check(&Token::True) {
                        self.advance();
                        Expression::Variable { name: "true".into(), span: self.previous_span() }
                    } else if self.check(&Token::False) {
                        self.advance();
                        Expression::Variable { name: "false".into(), span: self.previous_span() }
                    } else if self.is_identifier_or_keyword() {
                        let name = self.expect_namespaced_identifier_string()?;
                        Expression::Variable { name: name.into(), span: self.previous_span() }
                    } else {
                        self.parse_expression()?
                    };
                    props.insert(field_name.into(), expr);
                }

                self.expect(&Token::Newline)?;
            }

            self.expect(&Token::Dedent)?;
            (props, net_in_block)
        } else {
            // No properties block, just consume newline
            self.skip_whitespace();
            (rustc_hash::FxHashMap::default(), None)
        };

        let end_pos = self.previous_span().end;

        Ok(ContactPlacement {
            material: material.into(),
            name,
            position,
            from_elevation,
            to_elevation,
            net: net.or(net_in_block),
            properties,
            span: Span::new(start_pos, end_pos),
        })
    }
}
