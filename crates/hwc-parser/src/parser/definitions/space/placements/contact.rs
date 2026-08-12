use crate::ast::arena::ContactId;
use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse contact/via placement:
    /// - Absolute: `add contact(Tungsten) at [x:500um, y:325um] spanning z:6 to z:8`
    /// - Relational: `add contact(Tungsten) at: Region.center spanning layer: l1 to l2`
    ///
    /// Returns arena-allocated reference for zero-copy AST
    pub(in crate::parser) fn parse_contact(&mut self) -> Result<ContactId, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Contact)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        // v0.2.0: Named is required for contacts (no unnamed contacts)
        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.skip_whitespace();

        // Optional: net: NetName (before placement clauses)
        let net = if self.check(&Token::Identifier("net".into())) {
            self.advance(); // consume 'net'
            self.expect(&Token::Colon)?;
            Some(self.parse_net_name()?)
        } else {
            None
        };

        // Check for brace-grouped placement clauses or inline placement
        let (position, relational_anchor, from_elevation, to_elevation) =
            if self.check(&Token::OpenBrace) {
                // Multi-line syntax with braces: { at: ... \n spanning ... }
                self.advance(); // consume '{'
                self.skip_whitespace();
                if self.check(&Token::Newline) {
                    self.advance();
                }
                if self.check(&Token::Indent) {
                    self.advance();
                }

                self.expect(&Token::At)?;

                let (pos, anchor) = if self.check(&Token::Colon) {
                    self.advance();
                    let anchor = self.parse_region_anchor()?;
                    (None, Some(anchor))
                } else {
                    let pos = self.parse_coordinate_optional_z()?;
                    (Some(pos), None)
                };

                self.skip_whitespace();
                if self.check(&Token::Newline) {
                    self.advance();
                }

                self.expect(&Token::Spanning)?;
                let (from_elev, to_elev) = self.parse_spanning_clause()?;

                self.skip_whitespace();
                if self.check(&Token::Dedent) {
                    self.advance();
                }
                self.skip_whitespace();
                self.expect(&Token::CloseBrace)?;
                self.skip_whitespace();

                (pos, anchor, from_elev, to_elev)
            } else {
                // Inline syntax: at ... spanning ... OR just spanning with relational constraints
                // v0.2.1: Make 'at' optional if relational constraints are provided in properties block
                let (pos, anchor) = if self.check(&Token::At) {
                    self.advance(); // consume 'at'

                    if self.check(&Token::Colon) {
                        self.advance();
                        let anchor = self.parse_region_anchor()?;
                        (None, Some(anchor))
                    } else {
                        let pos = self.parse_coordinate_optional_z()?;
                        (Some(pos), None)
                    }
                } else {
                    // No 'at' clause - position will come from relational constraints
                    (None, None)
                };

                self.expect(&Token::Spanning)?;
                let (from_elev, to_elev) = self.parse_spanning_clause()?;

                (pos, anchor, from_elev, to_elev)
            };

        // Optional: properties block
        let (properties, net_in_block, relational_constraints) = if self.check(&Token::Colon) {
            self.advance();
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            let mut props = rustc_hash::FxHashMap::default();
            let mut net_in_block = None;
            let mut relational_constraints = smallvec::SmallVec::new();

            while !self.is_at_end() && !self.check(&Token::Dedent) {
                if self.check(&Token::Newline) {
                    self.advance();
                    continue;
                }

                let field_name = self.expect_identifier_or_keyword_string()?;
                self.expect(&Token::Colon)?;

                if field_name == "net" {
                    net_in_block = Some(self.parse_net_name()?);
                } else if field_name == "at" {
                    // v0.2.1: Support at: [x: expr, y: expr] in properties block
                    // This enables anchor arithmetic: at: [x: Contact_A.center_x, y: Contact_A.center_y]
                    // Must be used INSTEAD of inline 'at' placement (not in addition to)
                    let start_pos = self.current_span().start;
                    let coord = self.parse_coordinate_optional_z()?;
                    let end_pos = self.previous_span().end;
                    props.insert(
                        "at".into(),
                        Expression::Coordinate {
                            coord: Box::new(coord),
                            span: Span::new(start_pos, end_pos),
                        },
                    );
                } else if field_name == "align" {
                    // v0.2.1: Parse alignment constraints
                    let start_pos = self.current_span().start;
                    let axis_name = self.expect_identifier()?;
                    let axis = match axis_name.as_str() {
                        "center" => AlignmentAxis::Center,
                        "x" | "center_x" => AlignmentAxis::X,
                        "y" | "center_y" => AlignmentAxis::Y,
                        "z" | "center_z" => AlignmentAxis::Z,
                        "left" => AlignmentAxis::Left,
                        "right" => AlignmentAxis::Right,
                        "top" => AlignmentAxis::Top,
                        "bottom" => AlignmentAxis::Bottom,
                        _ => {
                            return Err(self.error(&format!(
                                "Unknown alignment axis: {}. Expected: center, x, y, z, left, right, top, or bottom",
                                axis_name
                            )))
                        }
                    };

                    self.expect(&Token::With)?;

                    // Parse target (entity name or expression)
                    let target = if self.check(&Token::OpenParen) {
                        let expr = self.parse_expression()?;
                        AlignmentTarget::Expression(expr)
                    } else if self
                        .current()
                        .map(|t| matches!(t.token, Token::Identifier(_)))
                        .unwrap_or(false)
                    {
                        let checkpoint = self.current;
                        let _ = self.expect_identifier_string()?;

                        if self.check(&Token::Dot) {
                            // Anchor reference - parse as expression
                            self.current = checkpoint;
                            let expr = self.parse_expression()?;
                            AlignmentTarget::Expression(expr)
                        } else {
                            // Simple entity name
                            self.current = checkpoint;
                            let component_name = self.parse_component_name()?;
                            AlignmentTarget::Entity(component_name)
                        }
                    } else {
                        return Err(self.error("Expected entity name or expression after 'with'"));
                    };

                    let span = Span::new(start_pos, self.previous_span().end);
                    relational_constraints.push(RelationalConstraint::Align { axis, target, span });
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
            (props, net_in_block, relational_constraints)
        } else {
            // No properties block, just consume newline
            self.skip_whitespace();
            (
                rustc_hash::FxHashMap::default(),
                None,
                smallvec::SmallVec::new(),
            )
        };

        let end_pos = self.previous_span().end;

        // Arena-allocate and return ID
        let contact = ContactPlacement {
            material: material.into(),
            name,
            position,
            relational_anchor,
            from_elevation,
            to_elevation,
            net: net.or(net_in_block),
            properties,
            relational_constraints, // v0.2.1: Pass relational constraints
            contour: None,
            span: Span::new(start_pos, end_pos),
        };

        Ok(self.arena.alloc_contact(contact))
    }

    /// Parse region anchor: `Region.center`, `Region.bottom_left`, etc.
    fn parse_region_anchor(&mut self) -> Result<RelationalAnchor, ParseError> {
        let start_pos = self.current_span().start;
        let region_name = self.expect_identifier()?;
        self.expect(&Token::Dot)?;
        let anchor_str = self.expect_identifier_string()?;

        let anchor_point = match anchor_str.as_str() {
            "center" => AnchorPoint::Center,
            "bottom_left" => AnchorPoint::BottomLeft,
            "bottom_right" => AnchorPoint::BottomRight,
            "top_left" => AnchorPoint::TopLeft,
            "top_right" => AnchorPoint::TopRight,
            "center_left" => AnchorPoint::CenterLeft,
            "center_right" => AnchorPoint::CenterRight,
            "top_center" => AnchorPoint::TopCenter,
            "bottom_center" => AnchorPoint::BottomCenter,
            _ => {
                return Err(self.error(&format!(
                    "Invalid anchor point '{}'. Expected: center, bottom_left, bottom_right, top_left, top_right, center_left, center_right, top_center, or bottom_center",
                    anchor_str
                )))
            }
        };

        let end_pos = self.previous_span().end;
        Ok(RelationalAnchor {
            region_name,
            anchor_point,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse spanning clause: `spanning layer: l1 to l2` or `spanning z: A to z: B`
    fn parse_spanning_clause(&mut self) -> Result<(Elevation, Elevation), ParseError> {
        if self.check(&Token::Identifier("layer".into())) {
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

            Ok((from_elev, to_elev))
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

            Ok((from_elev, to_elev))
        }
    }
}
