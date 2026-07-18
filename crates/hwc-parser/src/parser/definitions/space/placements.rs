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
        let mut thickness = None;
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
                    // Support both: [from] to [to] (rectangle) and Circle([x:0, y:0], radius) (circle)
                    if self.check(&Token::Identifier("Circle".into())) {
                        self.advance(); // consume 'Circle'
                        self.expect(&Token::OpenParen)?;
                        let center = self.parse_coordinate_optional_z()?;
                        self.expect(&Token::Comma)?;
                        let radius_expr = self.parse_expression()?;
                        self.expect(&Token::CloseParen)?;
                        boundary = Some(crate::PourBoundary::Circle {
                            center: Box::new(center),
                            radius: radius_expr,
                        });
                    } else {
                        let from = self.parse_coordinate_optional_z()?;
                        self.expect(&Token::To)?;
                        let to = self.parse_coordinate_optional_z()?;
                        boundary = Some(crate::PourBoundary::Rect(Box::new(from), Box::new(to)));
                    }
                }
                "thickness" => {
                    thickness = Some(self.parse_expression()?);
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
                        return Err(
                            self.error("Expected 'true', 'false', or '[list]' for merge property")
                        );
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
            name,
            elevation,
            thickness,
            boundary,
            net,
            device,
            thermal_relief,
            waivers,
            relational_constraints: smallvec::SmallVec::new(),
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
            name,
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
            let from_elev = if from_name.as_str() == "self" {
                Elevation::Relative
            } else {
                Elevation::Semantic(from_name)
            };

            self.expect(&Token::To)?;

            // Consume optional second "layer" keyword
            if self.check(&Token::Identifier("layer".into())) {
                self.advance();
                self.expect(&Token::Colon)?;
            }
            let to_name = self.expect_identifier()?;
            let to_elev = if to_name.as_str() == "self" {
                Elevation::Relative
            } else {
                Elevation::Semantic(to_name)
            };

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
                Elevation::Physical {
                    start: self.parse_expression()?,
                    end: None,
                }
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
                Elevation::Physical {
                    start: self.parse_expression()?,
                    end: None,
                }
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
                        Expression::Variable {
                            name: "true".into(),
                            span: self.previous_span(),
                        }
                    } else if self.check(&Token::False) {
                        self.advance();
                        Expression::Variable {
                            name: "false".into(),
                            span: self.previous_span(),
                        }
                    } else if self.is_identifier_or_keyword() {
                        let name = self.expect_namespaced_identifier_string()?;
                        Expression::Variable {
                            name: name.into(),
                            span: self.previous_span(),
                        }
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
            contour: None,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse plane placement: `add plane(Copper) named GND_Plane on layer: l1:`
    ///
    /// v0.1.9: Supports shape references and relational constraints
    /// New syntax: `add plane(Aluminum) named Pad_A: shape: Pad(1um, 1um) on layer: metal1 at: [x: 500nm, y: 500nm]`
    pub(in crate::parser) fn parse_plane(&mut self) -> Result<PlanePlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Plane)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        // v0.1.9: Parse relational constraints (align, above, below, right_of, left_of)
        let mut relational_constraints = smallvec::smallvec![];

        // Parse align constraint: align: <axis> with <target>
        if self.check(&Token::Align) {
            self.advance(); // consume 'align'
            self.expect(&Token::Colon)?;
            let axis_str = self.expect_identifier_string()?;
            let axis = match axis_str.as_str() {
                "center_x" => crate::ast::AlignmentAxis::CenterX,
                "center_y" => crate::ast::AlignmentAxis::CenterY,
                "center_z" => crate::ast::AlignmentAxis::CenterZ,
                "top" => crate::ast::AlignmentAxis::Top,
                "bottom" => crate::ast::AlignmentAxis::Bottom,
                "left" => crate::ast::AlignmentAxis::Left,
                "right" => crate::ast::AlignmentAxis::Right,
                _ => {
                    return Err(self.error(&format!(
                        "Invalid alignment axis '{}'. Expected: center_x, center_y, center_z, top, bottom, left, or right",
                        axis_str
                    )))
                }
            };
            self.expect(&Token::With)?;
            let target = self.parse_component_name()?;
            let span = Span::new(start_pos, self.previous_span().end);
            relational_constraints.push(crate::ast::RelationalConstraint::Align {
                axis,
                target,
                span,
            });
        }

        // Parse directional constraints
        loop {
            if self.check(&Token::Above)
                || self.check(&Token::Below)
                || self.check(&Token::RightOf)
                || self.check(&Token::LeftOf)
            {
                let constraint = if self.check(&Token::Above) {
                    self.advance();
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance();
                        self.expect_identifier()?;
                        self.expect(&Token::Colon)?;
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    crate::ast::RelationalConstraint::Directional(
                        crate::ast::DirectionalConstraint::Above { target, spacing },
                    )
                } else if self.check(&Token::Below) {
                    self.advance();
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance();
                        self.expect_identifier()?;
                        self.expect(&Token::Colon)?;
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    crate::ast::RelationalConstraint::Directional(
                        crate::ast::DirectionalConstraint::Below { target, spacing },
                    )
                } else if self.check(&Token::RightOf) {
                    self.advance();
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance();
                        self.expect_identifier()?;
                        self.expect(&Token::Colon)?;
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    crate::ast::RelationalConstraint::Directional(
                        crate::ast::DirectionalConstraint::RightOf { target, spacing },
                    )
                } else {
                    self.advance(); // consume 'left_of'
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance();
                        self.expect_identifier()?;
                        self.expect(&Token::Colon)?;
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    crate::ast::RelationalConstraint::Directional(
                        crate::ast::DirectionalConstraint::LeftOf { target, spacing },
                    )
                };
                relational_constraints.push(constraint);
            } else {
                break;
            }
        }

        self.expect(&Token::On)?;

        // Z-Axis Abstraction: support both physical (z:) and semantic (layer:)
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
                return Err(self.error("Expected 'z' or 'layer' for plane elevation"));
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

        let mut from = None;
        let mut to = None;
        let mut net = None;
        let mut thickness = None;
        let mut cutouts = Vec::new();
        let mut shape = None;

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            let field_name = self.expect_identifier_or_keyword_string()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "shape" => {
                    // shape: Pad(w: 600nm, h: 600nm)
                    let shape_start = self.current_span().start;
                    let shape_name = self.expect_identifier_string()?;
                    let mut parameters = smallvec::smallvec![];

                    if self.check(&Token::OpenParen) {
                        self.advance();
                        while !self.check(&Token::CloseParen) && !self.is_at_end() {
                            // Parse keyword parameter: name: value
                            let name = self.expect_identifier_string()?;
                            self.expect(&Token::Colon)?;

                            // Parse the value as a measurement expression
                            let expr = self.parse_expression()?;
                            let value = match expr {
                                crate::ast::Expression::Measurement { value, unit, .. } => {
                                    crate::ast::ParameterValue::Measurement(
                                        crate::ast::Measurement {
                                            value,
                                            unit,
                                            span: crate::lexer::Span { start: 0, end: 0 },
                                        },
                                    )
                                }
                                crate::ast::Expression::Literal { value, .. } => {
                                    crate::ast::ParameterValue::Number(value as f64)
                                }
                                crate::ast::Expression::FloatLiteral { value, .. } => {
                                    crate::ast::ParameterValue::Number(value)
                                }
                                _ => {
                                    return Err(self.error(
                                        "Expected measurement or number for shape parameter",
                                    ));
                                }
                            };

                            let param = crate::ast::Parameter::Keyword {
                                name: name.into(),
                                value,
                            };
                            parameters.push(param);

                            if self.check(&Token::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(&Token::CloseParen)?;
                    }

                    let shape_end = self.previous_span().end;
                    shape = Some(crate::ast::ShapeInstance {
                        shape_name: shape_name.into(),
                        parameters,
                        span: Span::new(shape_start, shape_end),
                    });
                }
                "at" => {
                    from = Some(self.parse_coordinate_optional_z()?);
                }
                "spanning" => {
                    // spanning layer: X to Y  OR  spanning [from] to [to]
                    if self.check(&Token::Identifier("layer".into())) {
                        self.advance(); // consume "layer"
                        self.expect(&Token::Colon)?;
                        let _layer_from = self.expect_identifier()?;
                        self.expect(&Token::To)?;
                        if self.check(&Token::Identifier("layer".into())) {
                            self.advance();
                            self.expect(&Token::Colon)?;
                        }
                        let _layer_to = self.expect_identifier()?;
                        // For now, store spanning info as optional coordinates
                        // The semantic layer resolution happens at a later stage
                        from = Some(self.parse_coordinate_optional_z()?);
                        self.expect(&Token::To)?;
                        to = Some(self.parse_coordinate_optional_z()?);
                    } else {
                        from = Some(self.parse_coordinate_optional_z()?);
                        self.expect(&Token::To)?;
                        to = Some(self.parse_coordinate_optional_z()?);
                    }
                }
                "thickness" => {
                    thickness = Some(self.parse_expression()?);
                }
                "net" => {
                    net = Some(self.parse_net_name()?);
                }
                "cutouts" => {
                    self.expect(&Token::Newline)?;
                    self.expect(&Token::Indent)?;

                    while !self.is_at_end() && !self.check(&Token::Dedent) {
                        if self.check(&Token::Newline) {
                            self.advance();
                            continue;
                        }

                        // Parse cutout shape: Rectangle(w, h) at [pos]  OR  Circle(r) at [pos]
                        let shape = if self.check(&Token::Identifier("Rectangle".into())) {
                            self.advance(); // consume 'Rectangle'
                            self.expect(&Token::OpenParen)?;
                            let width = self.parse_expression()?;
                            self.expect(&Token::Comma)?;
                            let height = self.parse_expression()?;
                            self.expect(&Token::CloseParen)?;
                            self.expect(&Token::At)?;
                            let at = self.parse_coordinate_optional_z()?;
                            CutoutShape::Rectangle { width, height, at }
                        } else if self.check(&Token::Identifier("Circle".into())) {
                            self.advance(); // consume 'Circle'
                            self.expect(&Token::OpenParen)?;
                            let radius = self.parse_expression()?;
                            self.expect(&Token::CloseParen)?;
                            self.expect(&Token::At)?;
                            let at = self.parse_coordinate_optional_z()?;
                            CutoutShape::Circle { radius, at }
                        } else {
                            return Err(
                                self.error("Expected 'Rectangle' or 'Circle' for cutout shape")
                            );
                        };

                        cutouts.push(shape);
                        self.skip_whitespace();
                    }

                    self.expect(&Token::Dedent)?;
                }
                _ => {
                    return Err(self.error(&format!("Unknown plane property: '{}'", field_name)));
                }
            }

            self.expect(&Token::Newline)?;
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(PlanePlacement {
            material: material.into(),
            name,
            shape,
            elevation,
            thickness,
            from,
            to,
            net,
            cutouts,
            relational_constraints,
            span: Span::new(start_pos, end_pos),
        })
    }
}
