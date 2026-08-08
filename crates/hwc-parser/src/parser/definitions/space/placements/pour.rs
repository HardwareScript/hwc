use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse pour placement: `add pour(Copper) named GND_Plane inside: RegionName on z:2:`
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

        // v0.2.0: Optional inside: RegionName
        let inside_region = if self.check(&Token::Inside) {
            self.advance();
            self.expect(&Token::Colon)?;
            Some(self.expect_identifier()?)
        } else {
            None
        };

        let elevation = if self.check(&Token::On) {
            self.advance();

            if self.check(&Token::Identifier("layer".into())) {
                self.advance(); // consume "layer"
                self.expect(&Token::Colon)?;
                let layer_name = self.expect_identifier()?;
                if layer_name.as_str() == "self" {
                    Elevation::Relative
                } else {
                    Elevation::Semantic(layer_name)
                }
            } else {
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
            }
        } else {
            Elevation::Relative
        };

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut boundary = None;
        let mut dimensions = None;
        let mut position = None;
        let mut net = None;
        let mut thickness = None;
        let mut device = None;
        let mut thermal_relief = false;
        let mut waivers = Waivers::default();
        let mut relational_constraints = smallvec::SmallVec::new();

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
                "dimensions" => {
                    // v0.2.1: Support dimensions: WIDTHxHEIGHT format
                    // Example: dimensions: 500nm by 600nm
                    let width = self.parse_expression()?;
                    self.expect(&Token::By)?;
                    let height = self.parse_expression()?;
                    dimensions = Some((width, height));
                }
                "at" => {
                    // v0.2.1: Center position for dimension-based pours
                    // Example: at: [x: 650nm, y: 1000nm]
                    position = Some(self.parse_coordinate_optional_z()?);
                }
                "align" => {
                    // v0.2.1: Alignment constraints
                    let start_pos = self.current_span().start;
                    let axis_name = self.expect_identifier()?;
                    let axis = match axis_name.as_str() {
                        "center" => AlignmentAxis::Center,
                        "x" => AlignmentAxis::X,
                        "y" => AlignmentAxis::Y,
                        "z" => AlignmentAxis::Z,
                        _ => {
                            return Err(self.error(&format!(
                                "Unknown alignment axis: {}. Expected: center, x, y, or z",
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
                }
                "above" | "below" | "left_of" | "right_of" => {
                    // v0.2.1: Directional constraints
                    let target = self.parse_component_name()?;

                    let spacing = if self.check(&Token::By) {
                        self.advance();
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };

                    let constraint = match field_name.as_str() {
                        "above" => DirectionalConstraint::Above { target, spacing },
                        "below" => DirectionalConstraint::Below { target, spacing },
                        "left_of" => DirectionalConstraint::LeftOf { target, spacing },
                        "right_of" => DirectionalConstraint::RightOf { target, spacing },
                        _ => unreachable!(),
                    };

                    relational_constraints.push(RelationalConstraint::Directional(constraint));
                }
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

        // v0.2.1: Extract width and height from dimensions tuple
        let (width, height) = match dimensions {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };

        // v0.2.1: Create boundary only for non-relative coordinates
        // For relative coordinates, store position + width/height and let compiler resolve
        let final_boundary = match (&position, &width, &height) {
            (Some(Coordinate::Relative(_)), Some(_), Some(_)) => {
                // Relative position with dimensions - don't create boundary yet

                boundary
            }
            (
                Some(pos @ (Coordinate::Declarative { .. } | Coordinate::Positional { .. })),
                Some(w),
                Some(h),
            ) => {
                // Declarative/Positional with dimensions - create boundary now

                let span_empty = Span::new(0, 0);
                let from = Coordinate::Positional {
                    x: Expression::Binary {
                        left: Box::new(pos.x().clone()),
                        operator: BinaryOperator::Subtract,
                        right: Box::new(Expression::Binary {
                            left: Box::new(w.clone()),
                            operator: BinaryOperator::Divide,
                            right: Box::new(Expression::Literal {
                                value: 2,
                                span: span_empty,
                            }),
                            span: span_empty,
                        }),
                        span: span_empty,
                    },
                    y: Expression::Binary {
                        left: Box::new(pos.y().clone()),
                        operator: BinaryOperator::Subtract,
                        right: Box::new(Expression::Binary {
                            left: Box::new(h.clone()),
                            operator: BinaryOperator::Divide,
                            right: Box::new(Expression::Literal {
                                value: 2,
                                span: span_empty,
                            }),
                            span: span_empty,
                        }),
                        span: span_empty,
                    },
                    z: pos.z().clone(),
                    span: span_empty,
                };
                let to = Coordinate::Positional {
                    x: Expression::Binary {
                        left: Box::new(pos.x().clone()),
                        operator: BinaryOperator::Add,
                        right: Box::new(Expression::Binary {
                            left: Box::new(w.clone()),
                            operator: BinaryOperator::Divide,
                            right: Box::new(Expression::Literal {
                                value: 2,
                                span: span_empty,
                            }),
                            span: span_empty,
                        }),
                        span: span_empty,
                    },
                    y: Expression::Binary {
                        left: Box::new(pos.y().clone()),
                        operator: BinaryOperator::Add,
                        right: Box::new(Expression::Binary {
                            left: Box::new(h.clone()),
                            operator: BinaryOperator::Divide,
                            right: Box::new(Expression::Literal {
                                value: 2,
                                span: span_empty,
                            }),
                            span: span_empty,
                        }),
                        span: span_empty,
                    },
                    z: pos.z().clone(),
                    span: span_empty,
                };
                Some(crate::PourBoundary::Rect(Box::new(from), Box::new(to)))
            }
            _ => boundary,
        };

        Ok(PourPlacement {
            material: material.into(),
            name,
            elevation,
            thickness,
            position, // v0.2.1: Store original position for relational resolver
            width,    // v0.2.1: Store width for compiler to resolve with relative position
            height,   // v0.2.1: Store height for compiler to resolve with relative position
            boundary: final_boundary,
            net,
            device,
            thermal_relief,
            waivers,
            relational_constraints, // v0.2.1: Pass relational constraints
            inside_region,          // v0.2.0: Region containment
            span: Span::new(start_pos, end_pos),
        })
    }
}
