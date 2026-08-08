//! Internal pour parsing for component-relative geometry (Sprint 2.2)

use super::super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};
use smallvec::SmallVec;

impl super::super::super::Parser {
    /// Parse internal pour within component layout block (Sprint 2.2)
    /// Syntax: `add pour(Material) named Name on z:Layer:`
    ///
    /// This is similar to space pour parsing but adapted for component-relative coordinates.
    /// The pour coordinates are relative to the component's origin and will be transformed
    /// to absolute coordinates during component placement.
    pub(super) fn parse_component_internal_pour(
        &mut self,
    ) -> Result<crate::ast::PourPlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Pour)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        self.expect(&Token::On)?;

        // v0.1.7 Z-Axis Abstraction: support both `on z:` and `on layer:`
        let elevation = if self.check(&Token::Identifier("layer".into())) {
            self.advance();
            self.expect(&Token::Colon)?;
            let layer_name = self.expect_identifier()?;
            if layer_name.as_str() == "self" {
                crate::ast::Elevation::Relative
            } else {
                crate::ast::Elevation::Semantic(layer_name)
            }
        } else {
            let coord_name = self.expect_identifier()?;
            if coord_name.as_str() != "z" {
                return Err(self.error("Expected 'z' or 'layer' for pour elevation"));
            }
            self.expect(&Token::Colon)?;

            if self.check(&Token::Identifier("relative".into())) {
                self.advance();
                crate::ast::Elevation::Relative
            } else {
                let start = self.parse_expression()?;

                let mut end = None;
                if self.check(&Token::To) {
                    self.advance(); // consume "to"
                    end = Some(self.parse_expression()?);
                }

                crate::ast::Elevation::Physical { start, end }
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
        let mut relational_constraints: SmallVec<[RelationalConstraint; 2]> = smallvec::smallvec![];

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // v0.1.9: Check for relational keywords (align, above, below, right_of, left_of)
            // These are standalone statements, not property: value pairs
            if self.check(&Token::Align) {
                self.advance(); // consume 'align'
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
                let target = self.parse_component_name()?;
                let span = Span::new(start_pos, self.previous_span().end);
                relational_constraints.push(RelationalConstraint::Align {
                    axis,
                    target: AlignmentTarget::Entity(target),
                    span,
                });
                self.skip_whitespace();
                continue;
            }

            if self.check_identifier("above")
                || self.check_identifier("below")
                || self.check_identifier("right_of")
                || self.check_identifier("left_of")
            {
                let constraint = if self.check_identifier("above") {
                    self.advance(); // consume 'above'
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance(); // consume 'with'
                        self.expect_identifier()?; // consume 'spacing'
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
                    self.advance(); // consume 'below'
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance(); // consume 'with'
                        self.expect_identifier()?; // consume 'spacing'
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
                    self.advance(); // consume 'right_of'
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance(); // consume 'with'
                        self.expect_identifier()?; // consume 'spacing'
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
                    self.advance(); // consume 'left_of'
                    let target = self.parse_component_name()?;
                    let spacing = if self.check(&Token::With) {
                        self.advance(); // consume 'with'
                        self.expect_identifier()?; // consume 'spacing'
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
                relational_constraints.push(constraint);
                self.skip_whitespace();
                continue;
            }

            // v0.1.6: Property names can be keywords (soft keywords)
            // This allows 'device' keyword to be used as property name: device: gate
            let field_name = self.expect_identifier_or_keyword_string()?;
            self.expect(&Token::Colon)?;

            match field_name.as_str() {
                "boundary" => {
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
                    // Sprint 2.2: For component internal pours, device is just a terminal name
                    // (not DeviceName.terminal like in spaces)
                    // Example: device: gate (refers to the component's gate terminal.into())
                    let terminal = self.expect_identifier_string()?;

                    // Create a DeviceBinding with empty device_name (will be filled during placement)
                    device = Some(crate::ast::DeviceBinding {
                        device_name: String::new().into(), // Empty for now, filled during component instantiation
                        terminal: terminal.into(),
                        span: Span::new(start_pos, self.previous_span().end),
                    });
                }
                "thermal_relief" => {
                    let val = self.expect_identifier()?;
                    thermal_relief = val.as_str() == "true";
                }
                _ => {
                    return Err(self.error(&format!("Unknown pour property: '{}'", field_name)));
                }
            }

            self.expect(&Token::Newline)?;
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(crate::ast::PourPlacement {
            material: material.into(),
            name,
            elevation,
            thickness,
            position: None, // Internal pours use boundary directly
            width: None,    // Internal pours use boundary directly
            height: None,   // Internal pours use boundary directly
            boundary,
            net,
            device,
            thermal_relief,
            waivers: crate::ast::Waivers::default(), // Internal pours use default waivers (no intentional overlaps by default)
            relational_constraints,
            inside_region: None, // Internal component pours don't belong to regions
            span: Span::new(start_pos, end_pos),
        })
    }
}
