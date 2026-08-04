//! Region parsing for floorplanning (v0.2.0)

use crate::{RegionAnchor, RegionBoundary, RegionConstraint, RegionConstraintType, RegionDefinition};
use crate::lexer::Token;
use crate::parser::ParseError;

impl crate::parser::Parser {
    /// Parse region definition:
    /// ```hw
    /// region AnalogRegion:
    ///     at: space.bottom_left + [100um, 100um]
    /// 
    /// region DigitalRegion:
    ///     right_of: AnalogRegion with spacing: pdk.min_spacing * 10
    ///     align: top with AnalogRegion
    /// ```
    pub(in crate::parser) fn parse_region(&mut self) -> Result<RegionDefinition, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::Region)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut anchor = None;
        let mut constraints = Vec::new();
        let mut boundary = None;

        // Parse region body
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check(&Token::At) {
                // Parse: at: space.bottom_left + [pdk.edge_clearance, pdk.edge_clearance]
                // or at: [x: 100um, y: 100um]
                self.advance();
                self.expect(&Token::Colon)?;

                

                if self.check(&Token::OpenBracket) {
                    let coord = self.parse_coordinate_optional_z()?;
                    anchor = Some(RegionAnchor::Absolute(coord));
                } else {
                    let expr = self.parse_prefix_expression()?;
                   
                    if self.check(&Token::Plus) || self.check(&Token::Hyphen) {
                        let op = if self.check(&Token::Plus) {
                            crate::ast::BinaryOperator::Add
                        } else {
                            crate::ast::BinaryOperator::Subtract
                        };
                        self.advance();
                      
                        let offset = self.parse_coordinate_optional_z()?;
                        anchor = Some(RegionAnchor::Offset {
                            base: expr,
                            operator: op,
                            offset,
                        });
                    } else {
                        anchor = Some(RegionAnchor::Expression(expr));
                    }
                }

                self.skip_whitespace();
            } else if self.check_identifier("right_of") {
                self.advance();
                self.expect(&Token::Colon)?;
                let target = self.expect_identifier()?;
                let spacing = self.parse_optional_spacing()?;
                constraints.push(RegionConstraint {
                    constraint_type: RegionConstraintType::RightOf,
                    target,
                    spacing,
                    span: self.previous_span(),
                });
                self.skip_whitespace();
            } else if self.check_identifier("left_of") {
                self.advance();
                self.expect(&Token::Colon)?;
                let target = self.expect_identifier()?;
                let spacing = self.parse_optional_spacing()?;
                constraints.push(RegionConstraint {
                    constraint_type: RegionConstraintType::LeftOf,
                    target,
                    spacing,
                    span: self.previous_span(),
                });
                self.skip_whitespace();
            } else if self.check_identifier("above") {
                self.advance();
                self.expect(&Token::Colon)?;
                let target = self.expect_identifier()?;
                let spacing = self.parse_optional_spacing()?;
                constraints.push(RegionConstraint {
                    constraint_type: RegionConstraintType::Above,
                    target,
                    spacing,
                    span: self.previous_span(),
                });
                self.skip_whitespace();
            } else if self.check_identifier("below") {
                self.advance();
                self.expect(&Token::Colon)?;
                let target = self.expect_identifier()?;
                let spacing = self.parse_optional_spacing()?;
                constraints.push(RegionConstraint {
                    constraint_type: RegionConstraintType::Below,
                    target,
                    spacing,
                    span: self.previous_span(),
                });
                self.skip_whitespace();
            } else if self.check(&Token::Align) {
                // Parse: align: top with AnalogRegion
                self.advance();
                self.expect(&Token::Colon)?;
                let align_type = self.expect_identifier()?;
                self.expect(&Token::With)?;
                let target = self.expect_identifier()?;

                let constraint_type = match align_type.as_str() {
                    "top" => RegionConstraintType::AlignTop,
                    "bottom" => RegionConstraintType::AlignBottom,
                    "left" => RegionConstraintType::AlignLeft,
                    "right" => RegionConstraintType::AlignRight,
                    "x" => RegionConstraintType::AlignX,
                    "y" => RegionConstraintType::AlignY,
                    _ => {
                        return Err(self.error(&format!(
                            "Unknown alignment type: '{}'. Expected 'top', 'bottom', 'left', 'right', 'x', or 'y'",
                            align_type
                        )))
                    }
                };

                constraints.push(RegionConstraint {
                    constraint_type,
                    target,
                    spacing: None,
                    span: self.previous_span(),
                });
                self.skip_whitespace();
            } else if self.check(&Token::Identifier("boundary".into())) {
                // Parse: boundary: [width: 500um, height: 300um]
                self.advance();
                self.expect(&Token::Colon)?;
                boundary = Some(self.parse_region_boundary()?);
                self.skip_whitespace();
            } else if self.check(&Token::Newline) {
                self.advance();
            } else {
                return Err(self.error(&format!(
                    "Unexpected token in region definition: {}",
                    self.current().map(|t| t.token.to_string()).unwrap_or_else(|| "EOF".to_string())
                )));
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(RegionDefinition {
            name,
            anchor,
            constraints,
            boundary,
            span: crate::lexer::Span::new(start_pos, end_pos),
        })
    }

    /// Parse optional spacing: `with spacing: pdk.min_spacing * 10`
    fn parse_optional_spacing(
        &mut self,
    ) -> Result<Option<crate::Expression>, ParseError> {
        if self.check(&Token::With) {
            self.advance();
            if self.check(&Token::Identifier("spacing".into())) {
                self.advance();
                self.expect(&Token::Colon)?;
                Ok(Some(self.parse_expression()?))
            } else {
                Err(self.error("Expected 'spacing' after 'with'"))
            }
        } else {
            Ok(None)
        }
    }

    /// Parse region boundary: `[width: 500um, height: 300um]`
    fn parse_region_boundary(&mut self) -> Result<RegionBoundary, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::OpenBracket)?;

        // Parse width
        self.expect(&Token::Identifier("width".into()))?;
        self.expect(&Token::Colon)?;
        let width = self.parse_expression()?;

        self.expect(&Token::Comma)?;

        // Parse height
        self.expect(&Token::Identifier("height".into()))?;
        self.expect(&Token::Colon)?;
        let height = self.parse_expression()?;

        self.expect(&Token::CloseBracket)?;

        let end_pos = self.previous_span().end;

        Ok(RegionBoundary {
            width,
            height,
            span: crate::lexer::Span::new(start_pos, end_pos),
        })
    }
}
