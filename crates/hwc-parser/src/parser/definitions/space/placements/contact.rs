use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse contact/via placement: 
    /// - Absolute: `add contact(Tungsten) at [x:500um, y:325um] spanning z:6 to z:8`
    /// - Relational: `add contact(Tungsten) at: Region.center spanning layer: l1 to l2`
    pub(in crate::parser) fn parse_contact(&mut self) -> Result<ContactPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Contact)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        // v0.2.0: Named is required for contacts (no unnamed contacts)
        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        // Optional: net: NetName
        let net = if self.check(&Token::Identifier("net".into())) {
            self.advance(); // consume 'net'
            self.expect(&Token::Colon)?;
            Some(self.parse_net_name()?)
        } else {
            None
        };

        self.expect(&Token::At)?;
        
        // v0.2.0: Support both absolute coordinates and relational anchors
        let (position, relational_anchor) = if self.check(&Token::Colon) {
            // Relational syntax: `at: Region.center`
            self.advance(); // consume ':'
            let anchor = self.parse_region_anchor()?;
            (None, Some(anchor))
        } else {
            // Absolute coordinate syntax: `at [x: ..., y: ...]`
            let pos = self.parse_coordinate_optional_z()?;
            (Some(pos), None)
        };

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
            relational_anchor,
            from_elevation,
            to_elevation,
            net: net.or(net_in_block),
            properties,
            contour: None,
            span: Span::new(start_pos, end_pos),
        })
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
}
