use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;
use smallvec::SmallVec;

impl<'ast> crate::parser::Parser<'ast> {
    // ========================================================================
    // STREAMLINED COMPONENT NAME PARSING (Zero String Re-Parsing!)
    // ========================================================================
    pub(in crate::parser) fn parse_component_name(&mut self) -> Result<ComponentName, ParseError> {
        let start_span = self.current_span().start;

        // Check for interpolated template name: L1_R{row}_C{col}
        if let Some(spanned) = self.current() {
            if let Token::InterpolatedIdentifier(ref parts) = spanned.token {
                let parts_clone = parts.clone();
                self.advance(); // Consume token

                let mut template_parts = Vec::with_capacity(parts_clone.len());
                for part in parts_clone {
                    match part {
                        crate::lexer::InterpolatedPart::Literal(lit) => {
                            template_parts.push(crate::ast::TemplateNamePart::Literal(lit.into()));
                        }
                        crate::lexer::InterpolatedPart::Expression(expr_str) => {
                            // Parse expression string directly using the main expression parser
                            let expr = self.parse_expression_from_str(&expr_str)?;
                            template_parts.push(crate::ast::TemplateNamePart::Expression(expr));
                        }
                    }
                }

                let span = Span::new(start_span, self.previous_span().end);
                return Ok(ComponentName::template(template_parts, span));
            }
        }

        // Regular identifier name
        let base_name = self.expect_identifier_string()?;

        // Optional array index suffix: Name[i]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            let span = Span::new(start_span, self.previous_span().end);
            Ok(ComponentName::indexed(base_name.into(), index_expr, span))
        } else {
            let span = Span::new(start_span, self.previous_span().end);
            Ok(ComponentName::simple(base_name.into(), span))
        }
    }

    /// Streamlined string expression parser using standard operator precedence
    fn parse_expression_from_str(
        &self,
        expr_str: &str,
    ) -> Result<crate::ast::Expression, ParseError> {
        let trimmed = expr_str.trim();

        // Fast-path: Literal integer
        if let Ok(val) = trimmed.parse::<i64>() {
            return Ok(crate::ast::Expression::Literal {
                value: val,
                span: self.current_span(),
            });
        }

        // Fast-path: Literal float
        if let Ok(val) = trimmed.parse::<f64>() {
            return Ok(crate::ast::Expression::FloatLiteral {
                value: val,
                span: self.current_span(),
            });
        }

        // Fast-path: Simple Variable name
        if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Ok(crate::ast::Expression::Variable {
                name: trimmed.to_string().into(),
                span: self.current_span(),
            });
        }

        // Complex expressions: Delegate to compound parser
        self.parse_compound_expr_str(trimmed)
    }

    fn parse_compound_expr_str(&self, s: &str) -> Result<crate::ast::Expression, ParseError> {
        // Evaluate additions/subtractions outside parens
        if let Some((left, op, right)) = self.split_outer_operator(s, &['+', '-']) {
            let operator = if op == '+' {
                crate::ast::BinaryOperator::Add
            } else {
                crate::ast::BinaryOperator::Subtract
            };
            return Ok(crate::ast::Expression::Binary {
                left: Box::new(self.parse_expression_from_str(left)?),
                operator,
                right: Box::new(self.parse_expression_from_str(right)?),
                span: self.current_span(),
            });
        }

        // Evaluate multiplications/divisions outside parens
        if let Some((left, op, right)) = self.split_outer_operator(s, &['*', '/']) {
            let operator = if op == '*' {
                crate::ast::BinaryOperator::Multiply
            } else {
                crate::ast::BinaryOperator::Divide
            };
            return Ok(crate::ast::Expression::Binary {
                left: Box::new(self.parse_expression_from_str(left)?),
                operator,
                right: Box::new(self.parse_expression_from_str(right)?),
                span: self.current_span(),
            });
        }

        // Handle parentheses: (expr)
        if s.starts_with('(') && s.ends_with(')') {
            return Ok(crate::ast::Expression::Grouped {
                expression: Box::new(self.parse_expression_from_str(&s[1..s.len() - 1])?),
                span: self.current_span(),
            });
        }

        Err(self.error(&format!("Invalid expression in interpolation: '{}'", s)))
    }

    fn split_outer_operator<'a>(
        &self,
        s: &'a str,
        ops: &[char],
    ) -> Option<(&'a str, char, &'a str)> {
        let mut depth = 0;
        let mut last_match = None;

        for (i, ch) in s.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ if depth == 0 && i > 0 && ops.contains(&ch) => {
                    last_match = Some((i, ch));
                }
                _ => {}
            }
        }

        last_match.map(|(pos, ch)| (&s[..pos], ch, &s[pos + 1..]))
    }

    // ========================================================================
    // UNORDERED COMPONENT PLACEMENT PARSER (Zero Lookahead Hacks!)
    // ========================================================================

    /// Parse component placement: `add Type (params) named Instance at [Z,X,Y] rotated angle`
    /// v0.2.1: Supports unordered placement clauses (at, on, rotated, align, directional)
    pub(in crate::parser) fn parse_component_placement(
        &mut self,
    ) -> Result<ComponentPlacement, ParseError> {
        // Enter component placement context for better error messages
        self.error_context
            .enter_context(crate::parser::ParsingContext::ComponentPlacement);

        // Track parsing state for intelligent error messages
        let mut state = crate::parser::PlacementParseState::default();

        let result = self.parse_component_placement_impl(&mut state);

        // Exit context
        self.error_context.exit_context();

        result
    }

    /// Internal implementation with state tracking
    fn parse_component_placement_impl(
        &mut self,
        state: &mut crate::parser::PlacementParseState,
    ) -> Result<ComponentPlacement, ParseError> {
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

        let mut position = None;
        let mut elevation = None;
        let mut rotation = None;
        let mut relational_constraints = SmallVec::new();

        // --------------------------------------------------------------------
        // UNORDERED CLAUSE DISPATCHER LOOP
        // Clauses (at:, on layer:, rotated, align:, directional) in ANY order!
        // --------------------------------------------------------------------
        loop {
            if self.check(&Token::At) {
                if position.is_some() {
                    return Err(self.error("Duplicate 'at' clause"));
                }
                self.advance();
                position = Some(self.parse_coordinate_optional_z()?);
                state.has_position = true;
            } else if self.check(&Token::On) {
                if elevation.is_some() {
                    return Err(self.error("Duplicate 'on' clause"));
                }
                self.advance();
                elevation = Some(self.parse_elevation_clause()?);
                state.has_elevation = true;
            } else if self.check(&Token::Rotated) {
                if rotation.is_some() {
                    return Err(self.error("Duplicate 'rotated' clause"));
                }
                rotation = Some(self.parse_rotation()?);
                state.has_rotation = true;
            } else if self.check(&Token::Align) {
                relational_constraints.push(self.parse_align_constraint(start_pos)?);
            } else if self.is_directional_preposition() {
                relational_constraints.push(self.parse_directional_constraint()?);
            } else {
                break; // No more placement clauses
            }
        }

        // Parse optional configuration block (v0.1.6)
        // Can contain:
        // - Array config (Sprint 3.2): layout, pitch, merge
        // - Net bindings (Item #13): net: [pin: NetName, ...]
        // - Mounting (v0.1.7): mount: top|bottom|embedded
        let mut array_config = None;
        let mut pin_net_bindings = rustc_hash::FxHashMap::default();
        let mut waivers = Waivers::default();
        let mut mount = None;
        let mut standoff = None;

        if self.check(&Token::Colon) {
            state.has_config_block = true;
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
                    array_config =
                        Some(self.parse_array_config(array_count.unwrap(), &mut waivers)?);
                    break; // Array config consumes until dedent
                } else if self.check_identifier("mount") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    let side_name = self.expect_identifier_string()?;
                    state.has_mount = true;
                    mount = Some(match side_name.as_str() {
                        "top" => MountingSide::Top,
                        "bottom" => MountingSide::Bottom,
                        "embedded" => MountingSide::Embedded,
                        _ => {
                            return Err(self.error(
                                "Invalid mounting side. Expected: top, bottom, or embedded",
                            ))
                        }
                    });
                    self.skip_whitespace();
                } else if self.check_identifier("standoff") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    state.has_standoff = true;
                    standoff = Some(self.parse_expression()?);
                    self.skip_whitespace();
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
                        return Err(
                            self.error("Expected 'true', 'false', or '[list]' for merge property")
                        );
                    }
                    self.skip_whitespace();
                } else if self.check_identifier("floating") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.floating = self.parse_boolean()?;
                    self.skip_whitespace();
                } else if self.check_identifier("isolated") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.isolated = self.parse_boolean()?;
                    self.skip_whitespace();
                } else if self.check_identifier("snap_to_surface") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.snap_to_surface = self.parse_boolean()?;
                    self.skip_whitespace();
                } else if self.check_identifier("virtual") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.virtual_component = self.parse_boolean()?;
                    self.skip_whitespace();
                } else if self.check_identifier("locked") {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    waivers.locked = self.parse_boolean()?;
                    self.skip_whitespace();
                } else if self.check_identifier("net") {
                    // Net bindings (Item #13)
                    self.advance(); // consume 'net'
                    self.expect(&Token::Colon)?;
                    state.has_net_bindings = true;
                    pin_net_bindings = self.parse_net_bindings()?;
                    self.skip_whitespace();
                } else if self.check_identifier("allow_substrate_overlap") {
                    // LEGACY SUPPORT: Map to waivers.merge
                    self.advance();
                    self.expect(&Token::Colon)?;
                    if self.parse_boolean()? {
                        waivers.merge = MergeWaiver::All;
                    }
                    self.skip_whitespace();
                } else {
                    // Use context-aware error generation
                    if let Some(current) = self.current() {
                        return Err(self.error_context.unexpected_token_error(
                            &current.token,
                            &current.span,
                            Some(state),
                        ));
                    } else {
                        return Err(self.error("Unexpected end of file in component configuration"));
                    }
                }
            }

            self.expect(&Token::Dedent)?;

            // If array_count was provided but no layout/pitch block was found
            // (e.g. the config block only had 'net:', 'mount:', etc.), create
            // a default array_config so the array is still properly expanded.
            if let Some(count) = array_count {
                if array_config.is_none() {
                    array_config = Some(crate::ast::ArrayConfig {
                        count,
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
            }
        } else if let Some(count) = array_count {
            // Array syntax without config block - use defaults
            array_config = Some(crate::ast::ArrayConfig {
                count,
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

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(ComponentPlacement {
            component_type,
            parameters,
            name,
            position,
            rotation,
            elevation,
            mount,
            standoff,
            array_config,
            pin_net_bindings,
            waivers,
            relational_constraints,
            span: Span::new(start_pos, end_pos),
        })
    }

    // ========================================================================
    // HELPER DISPATCHERS (Clean, Consolidated Functions)
    // ========================================================================

    fn parse_elevation_clause(&mut self) -> Result<Elevation, ParseError> {
        if self.check_identifier("layer") {
            self.advance();
            self.expect(&Token::Colon)?;
            let layer_name = self.expect_identifier()?;
            if layer_name.as_str() == "self" {
                Ok(Elevation::Relative)
            } else {
                Ok(Elevation::Semantic(layer_name))
            }
        } else if self.check_identifier("z") {
            self.advance();
            self.expect(&Token::Colon)?;
            if self.check_identifier("relative") {
                self.advance();
                Ok(Elevation::Relative)
            } else {
                let start = self.parse_expression()?;
                let end = if self.check(&Token::To) {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Ok(Elevation::Physical { start, end })
            }
        } else {
            Err(self.error("Expected 'layer' or 'z' after 'on' keyword"))
        }
    }

    fn parse_align_constraint(
        &mut self,
        start_pos: usize,
    ) -> Result<RelationalConstraint, ParseError> {
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
            _ => return Err(self.error(&format!("Invalid alignment axis '{}'. Expected: center, x, y, z, top, bottom, left, or right", axis_str))),
        };
        self.expect(&Token::With)?;
        let target = self.parse_component_name()?;
        let span = Span::new(start_pos, self.previous_span().end);
        Ok(RelationalConstraint::Align {
            axis,
            target: AlignmentTarget::Entity(target),
            span,
        })
    }

    fn is_directional_preposition(&self) -> bool {
        self.check_identifier("above")
            || self.check_identifier("below")
            || self.check_identifier("right_of")
            || self.check_identifier("left_of")
    }

    fn parse_directional_constraint(&mut self) -> Result<RelationalConstraint, ParseError> {
        let dir = self.expect_identifier_string()?;
        let target = self.parse_component_name()?;
        let spacing = if self.check(&Token::With) {
            self.advance(); // consume 'with'
            self.expect_identifier()?; // consume 'spacing'
            self.expect(&Token::Colon)?;
            Some(self.parse_expression()?)
        } else {
            None
        };

        let constraint = match dir.as_str() {
            "above" => DirectionalConstraint::Above { target, spacing },
            "below" => DirectionalConstraint::Below { target, spacing },
            "right_of" => DirectionalConstraint::RightOf { target, spacing },
            "left_of" => DirectionalConstraint::LeftOf { target, spacing },
            _ => unreachable!(),
        };

        Ok(RelationalConstraint::Directional(constraint))
    }

    /// Parse array configuration block (indented)
    /// Expected to be called after consuming colon, newline, and indent tokens
    /// v0.1.6 Sprint 3.2: Parse array configuration with explicit merge intent
    fn parse_array_config(
        &mut self,
        count: usize,
        waivers: &mut Waivers,
    ) -> Result<crate::ast::ArrayConfig, ParseError> {
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
                self.skip_whitespace();
            } else if self.check_identifier("pitch") {
                self.advance();
                self.expect(&Token::Colon)?;
                pitch = Some(self.parse_measurement()?);
                self.skip_whitespace();
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
                    return Err(
                        self.error("Expected 'true', 'false', or '[list]' for merge property")
                    );
                }
                self.skip_whitespace();
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
    pub(in crate::parser) fn parse_array_terminal_list(
        &mut self,
    ) -> Result<SmallVec<[compact_str::CompactString; 2]>, ParseError> {
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
}
