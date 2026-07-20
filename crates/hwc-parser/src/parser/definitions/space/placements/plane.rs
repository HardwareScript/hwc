use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

impl crate::parser::Parser {
    /// Parse plane placement: `add plane(Copper) named GND_Plane on layer: l1:`
    pub(in crate::parser) fn parse_plane(&mut self) -> Result<PlanePlacement, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Add)?;
        self.expect(&Token::Plane)?;
        self.expect(&Token::OpenParen)?;
        let material = self.expect_namespaced_identifier_string()?;
        self.expect(&Token::CloseParen)?;

        self.expect(&Token::Named)?;
        let name = self.parse_component_name()?;

        let relational_constraints = self.parse_relational_constraints(start_pos)?;

        self.expect(&Token::On)?;

        let elevation = self.parse_elevation("plane")?;

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
            span: Span::new(start_pos, end_pos),
        })
    }

    fn parse_relational_constraints(
        &mut self,
        start_pos: usize,
    ) -> Result<smallvec::SmallVec<[RelationalConstraint; 2]>, ParseError> {
        let mut constraints = smallvec::smallvec![];

        if self.check(&Token::Align) {
            self.advance();
            self.expect(&Token::Colon)?;
            let axis_str = self.expect_identifier_string()?;
            let axis = match axis_str.as_str() {
                "center_x" => AlignmentAxis::CenterX,
                "center_y" => AlignmentAxis::CenterY,
                "center_z" => AlignmentAxis::CenterZ,
                "top" => AlignmentAxis::Top,
                "bottom" => AlignmentAxis::Bottom,
                "left" => AlignmentAxis::Left,
                "right" => AlignmentAxis::Right,
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
            constraints.push(RelationalConstraint::Align { axis, target, span });
        }

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
                    RelationalConstraint::Directional(DirectionalConstraint::Above {
                        target,
                        spacing,
                    })
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
                    RelationalConstraint::Directional(DirectionalConstraint::Below {
                        target,
                        spacing,
                    })
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
                let expr = self.parse_expression()?;
                let value = match expr {
                    Expression::Measurement { value, unit, .. } => {
                        ParameterValue::Measurement(Measurement {
                            value,
                            unit,
                            span: Span { start: 0, end: 0 },
                        })
                    }
                    Expression::Literal { value, .. } => ParameterValue::Number(value as f64),
                    Expression::FloatLiteral { value, .. } => ParameterValue::Number(value),
                    _ => {
                        return Err(self.error("Expected measurement or number for shape parameter"))
                    }
                };
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
