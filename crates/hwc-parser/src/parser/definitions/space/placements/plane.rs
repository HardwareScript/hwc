use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse plane placement: `add plane(Copper) named GND_Plane inside: RegionName on layer: l1:`
    pub(in crate::parser) fn parse_plane(&mut self) -> Result<PlanePlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Plane)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.skip_whitespace(); // Allow newline after name

        // v0.2.0: Optional inside: RegionName
        let inside_region = if self.check(&Token::Inside) {
            self.advance();
            self.expect(&Token::Colon)?;
            let region_id = self.expect_identifier()?;
            self.skip_whitespace(); // Allow newline after inside clause
            eprintln!(
                "[DBG plane] after inside: {:?} | next tok: {:?}",
                region_id.as_str(),
                self.current().map(|t| format!("{:?}", t.token))
            );
            Some(region_id)
        } else {
            None
        };

        eprintln!(
            "[DBG plane] before relational_constraints | current tok: {:?}",
            self.current().map(|t| format!("{:?}", t.token))
        );

        // Parse relational constraints - either inline or in braces
        let relational_constraints = if self.check(&Token::OpenBrace) {
            // Multi-line syntax with braces: { align: ... \n align: ... }
            self.advance(); // consume '{'
            self.skip_whitespace(); // Allow newline after '{'
            if self.check(&Token::Newline) {
                self.advance();
            }
            if self.check(&Token::Indent) {
                self.advance();
            }

            let constraints = self.parse_relational_constraints_block(start_pos)?;

            self.skip_whitespace();
            if self.check(&Token::Dedent) {
                self.advance();
            }
            self.skip_whitespace();
            self.expect(&Token::CloseBrace)?;
            self.skip_whitespace(); // Allow newline after '}'

            constraints
        } else {
            // Inline syntax: align: ... align: ... (on same line)
            self.parse_relational_constraints(start_pos)?
        };

        self.skip_whitespace(); // Allow newline after relational constraints
        eprintln!(
            "[DBG plane] after relational_constraints | current tok: {:?}",
            self.current().map(|t| format!("{:?}", t.token))
        );

        let elevation = if self.check(&Token::On) {
            self.advance();
            let elev = self.parse_elevation("plane")?;
            self.skip_whitespace(); // Allow newline after elevation
            eprintln!(
                "[DBG plane] elevation parsed, expecting block colon | next tok: {:?}",
                self.current().map(|t| format!("{:?}", t.token))
            );
            self.expect(&Token::Colon)?;
            elev
        } else {
            eprintln!(
                "[DBG plane] no 'on', expecting block colon | current tok: {:?}",
                self.current().map(|t| format!("{:?}", t.token))
            );
            self.expect(&Token::Colon)?;
            eprintln!(
                "[DBG plane] consumed block colon | current tok: {:?}",
                self.current().map(|t| format!("{:?}", t.token))
            );
            Elevation::Relative
        };

        eprintln!(
            "[DBG plane] before newline | current tok: {:?}",
            self.current().map(|t| format!("{:?}", t.token))
        );
        self.expect(&Token::Newline)?;
        eprintln!(
            "[DBG plane] before indent | current tok: {:?}",
            self.current().map(|t| format!("{:?}", t.token))
        );
        self.expect(&Token::Indent)?;
        eprintln!(
            "[DBG plane] entered body | current tok: {:?}",
            self.current().map(|t| format!("{:?}", t.token))
        );

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

            eprintln!(
                "[DBG plane] body loop | current tok: {:?}",
                self.current().map(|t| format!("{:?}", t.token))
            );
            let field_name = self.expect_identifier_or_keyword_string()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "shape" => {
                    shape = Some(self.parse_shape_instance()?);
                }
                "at" => {
                    from = Some(self.parse_coordinate_optional_z()?);
                }
                "spanning" => {
                    // spanning layer: X to Y  OR  spanning [from] to [to]
                    if self.check(&Token::Identifier("layer".into())) {
                        self.advance();
                        self.expect(&Token::Colon)?;
                        let _layer_from = self.expect_identifier()?;
                        self.expect(&Token::To)?;
                        if self.check(&Token::Identifier("layer".into())) {
                            self.advance();
                            self.expect(&Token::Colon)?;
                        }
                        let _layer_to = self.expect_identifier()?;
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

                        let cutout = if self.check(&Token::Identifier("Rectangle".into())) {
                            self.advance();
                            self.expect(&Token::OpenParen)?;
                            let width = self.parse_expression()?;
                            self.expect(&Token::Comma)?;
                            let height = self.parse_expression()?;
                            self.expect(&Token::CloseParen)?;
                            self.expect(&Token::At)?;
                            let at = self.parse_coordinate_optional_z()?;
                            CutoutShape::Rectangle { width, height, at }
                        } else if self.check(&Token::Identifier("Circle".into())) {
                            self.advance();
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

                        cutouts.push(cutout);
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
            inside_region, // v0.2.0: Region containment
            span: Span::new(start_pos, end_pos),
        })
    }

    fn parse_relational_constraints(
        &mut self,
        start_pos: usize,
    ) -> Result<smallvec::SmallVec<[RelationalConstraint; 2]>, ParseError> {
        let mut constraints = smallvec::smallvec![];

        // Parse multiple align constraints (align: center_x with A align: center_y with A)
        // v0.2.1: Now supports expressions: align: center_x with (A.center_x + B.center_x) / 2
        while self.check(&Token::Align) {
            self.advance();
            self.expect(&Token::Colon)?;
            let axis_str = self.expect_identifier_string()?;
            let axis = match axis_str.as_str() {
                "center" => AlignmentAxis::Center,
                "x" => AlignmentAxis::X,
                "y" => AlignmentAxis::Y,
                "z" => AlignmentAxis::Z,
                "top" => AlignmentAxis::Top,
                "bottom" => AlignmentAxis::Bottom,
                "left" => AlignmentAxis::Left,
                "right" => AlignmentAxis::Right,
                _ => {
                    return Err(self.error(&format!(
                        "Invalid alignment axis '{}'. Expected: center, x, y, z, top, bottom, left, or right",
                        axis_str
                    )))
                }
            };
            self.expect(&Token::With)?;

            // v0.2.1: Parse target as expression or simple entity name
            let target = if self.check(&Token::OpenParen) {
                // Complex expression: (A.center_x + B.center_x) / 2
                let expr = self.parse_expression()?;
                AlignmentTarget::Expression(expr)
            } else if self
                .current()
                .map(|t| matches!(t.token, Token::Identifier(_)))
                .unwrap_or(false)
            {
                // Check if it's a simple identifier or an anchor reference
                let checkpoint = self.current;
                let _ = self.expect_identifier_string()?;

                if self.check(&Token::Dot) {
                    // It's an anchor reference like Pad_A.center_x - parse as expression
                    self.current = checkpoint; // Backtrack
                    let expr = self.parse_expression()?;
                    AlignmentTarget::Expression(expr)
                } else {
                    // It's a simple entity name
                    self.current = checkpoint; // Backtrack
                    let component_name = self.parse_component_name()?;
                    AlignmentTarget::Entity(component_name)
                }
            } else {
                return Err(self.error("Expected entity name or expression after 'with'"));
            };

            let span = Span::new(start_pos, self.previous_span().end);
            constraints.push(RelationalConstraint::Align { axis, target, span });
        }

        // Parse multiple directional constraints (right_of A with spacing: X above B with spacing: Y)
        loop {
            if self.check_identifier("above")
                || self.check_identifier("below")
                || self.check_identifier("right_of")
                || self.check_identifier("left_of")
            {
                let constraint = if self.check_identifier("above") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::Above {
                        target,
                        spacing,
                    })
                } else if self.check_identifier("below") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::Below {
                        target,
                        spacing,
                    })
                } else if self.check_identifier("right_of") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::RightOf {
                        target,
                        spacing,
                    })
                } else {
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
                    RelationalConstraint::Directional(DirectionalConstraint::LeftOf {
                        target,
                        spacing,
                    })
                };
                constraints.push(constraint);
            } else {
                break;
            }
        }

        Ok(constraints)
    }

    /// Parse relational constraints inside braces (multi-line syntax)
    /// Handles: { align: center_x with A \n align: center_y with A }
    fn parse_relational_constraints_block(
        &mut self,
        start_pos: usize,
    ) -> Result<smallvec::SmallVec<[RelationalConstraint; 2]>, ParseError> {
        let mut constraints = smallvec::smallvec![];

        while !self.check(&Token::CloseBrace) && !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseBrace) || self.check(&Token::Dedent) {
                break;
            }

            // Parse alignment constraint: align: axis with target
            // v0.2.1: Now supports expressions: align: center_x with (A.center_x + B.center_x) / 2
            if self.check(&Token::Align) {
                self.advance();
                self.expect(&Token::Colon)?;
                let axis_str = self.expect_identifier_string()?;
                let axis = match axis_str.as_str() {
                    "center" => AlignmentAxis::Center,
                    "x" => AlignmentAxis::X,
                    "y" => AlignmentAxis::Y,
                    "z" => AlignmentAxis::Z,
                    "top" => AlignmentAxis::Top,
                    "bottom" => AlignmentAxis::Bottom,
                    "left" => AlignmentAxis::Left,
                    "right" => AlignmentAxis::Right,
                    _ => {
                        return Err(self.error(&format!(
                            "Invalid alignment axis '{}'. Expected: center, x, y, z, top, bottom, left, or right",
                            axis_str
                        )))
                    }
                };
                self.expect(&Token::With)?;

                // v0.2.1: Parse target as expression or simple entity name
                let target = if self.check(&Token::OpenParen) {
                    // Complex expression: (A.center_x + B.center_x) / 2
                    let expr = self.parse_expression()?;
                    AlignmentTarget::Expression(expr)
                } else if self
                    .current()
                    .map(|t| matches!(t.token, Token::Identifier(_)))
                    .unwrap_or(false)
                {
                    // Check if it's a simple identifier or an anchor reference
                    let checkpoint = self.current;
                    let _ = self.expect_identifier_string()?;

                    if self.check(&Token::Dot) {
                        // It's an anchor reference like Pad_A.center_x - parse as expression
                        self.current = checkpoint; // Backtrack
                        let expr = self.parse_expression()?;
                        AlignmentTarget::Expression(expr)
                    } else {
                        // It's a simple entity name
                        self.current = checkpoint; // Backtrack
                        let component_name = self.parse_component_name()?;
                        AlignmentTarget::Entity(component_name)
                    }
                } else {
                    return Err(self.error("Expected entity name or expression after 'with'"));
                };

                let span = Span::new(start_pos, self.previous_span().end);
                constraints.push(RelationalConstraint::Align { axis, target, span });
                self.skip_whitespace();
                continue;
            }

            // Parse directional constraint: above/below/right_of/left_of target [with spacing: expr]
            if self.check_identifier("above")
                || self.check_identifier("below")
                || self.check_identifier("right_of")
                || self.check_identifier("left_of")
            {
                let constraint = if self.check_identifier("above") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::Above {
                        target,
                        spacing,
                    })
                } else if self.check_identifier("below") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::Below {
                        target,
                        spacing,
                    })
                } else if self.check_identifier("right_of") {
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
                    RelationalConstraint::Directional(DirectionalConstraint::RightOf {
                        target,
                        spacing,
                    })
                } else {
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
                    RelationalConstraint::Directional(DirectionalConstraint::LeftOf {
                        target,
                        spacing,
                    })
                };
                constraints.push(constraint);
                self.skip_whitespace();
                continue;
            }

            // No more constraints recognized
            break;
        }

        Ok(constraints)
    }

    fn parse_elevation(&mut self, context: &str) -> Result<Elevation, ParseError> {
        if self.check(&Token::Identifier("layer".into())) {
            self.advance();
            self.expect(&Token::Colon)?;
            let layer_name = self.expect_identifier()?;
            if layer_name.as_str() == "self" {
                Ok(Elevation::Relative)
            } else {
                Ok(Elevation::Semantic(layer_name))
            }
        } else {
            let coord_name = self.expect_identifier()?;
            if coord_name.as_str() != "z" {
                return Err(self.error(&format!(
                    "Expected 'z' or 'layer' for {} elevation",
                    context
                )));
            }
            self.expect(&Token::Colon)?;

            if self.check(&Token::Identifier("relative".into())) {
                self.advance();
                Ok(Elevation::Relative)
            } else {
                let start = self.parse_expression()?;
                let mut end = None;
                if self.check(&Token::To) {
                    self.advance();
                    end = Some(self.parse_expression()?);
                }
                Ok(Elevation::Physical { start, end })
            }
        }
    }

    fn parse_shape_instance(&mut self) -> Result<ShapeInstance, ParseError> {
        let shape_start = self.current_span().start;
        let shape_name = self.expect_identifier_string()?;
        let mut parameters = smallvec::smallvec![];
        if self.check(&Token::OpenParen) {
            self.advance();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                let name = self.expect_identifier_string()?;
                self.expect(&Token::Colon)?;
                // v0.1.10: Parse full expression (supports variables, operations, literals)
                let expr = self.parse_expression()?;
                let value = ParameterValue::Expression(expr);
                parameters.push(Parameter::Keyword {
                    name: name.into(),
                    value,
                });
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(&Token::CloseParen)?;
        }
        let shape_end = self.previous_span().end;
        Ok(ShapeInstance {
            shape_name: shape_name.into(),
            parameters,
            span: Span::new(shape_start, shape_end),
        })
    }
}
