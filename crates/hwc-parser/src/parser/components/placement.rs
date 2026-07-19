use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;
use smallvec::SmallVec;

impl crate::parser::Parser {
    pub(in crate::parser) fn parse_component_name(&mut self) -> Result<ComponentName, ParseError> {
        let name_span_start = self.current_span().start;
        let base_name = self.expect_identifier_string()?;

        // Check for array index: [i] or [0]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            let name_span_end = self.previous_span().end;

            Ok(crate::ast::ComponentName::indexed(
                base_name.into(),
                index_expr,
                crate::lexer::Span::new(name_span_start, name_span_end),
            ))
        } else {
            let name_span_end = self.previous_span().end;
            Ok(crate::ast::ComponentName::simple(
                base_name.into(),
                crate::lexer::Span::new(name_span_start, name_span_end),
            ))
        }
    }

    /// Parse component placement: `add Type (params) named Instance at [Z,X,Y] rotated angle`
    /// v0.1.6 Sprint 3.2: Supports array syntax: `add Type[count] named ArrayName`
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

        // v0.1.9: Parse optional 'on layer: l1' or 'on z: 1mm' prepositional syntax
        // This can appear before or after position, so check here first
        let elevation = if self.check(&Token::On) {
            self.advance(); // consume 'on'
            if self.check_identifier("layer") {
                self.advance(); // consume 'layer'
                self.expect(&Token::Colon)?;
                let layer_name = self.expect_identifier()?;
                state.has_elevation = true;
                if layer_name.as_str() == "self" {
                    Some(Elevation::Relative)
                } else {
                    Some(Elevation::Semantic(layer_name))
                }
            } else if self.check_identifier("z") {
                self.advance(); // consume 'z'
                self.expect(&Token::Colon)?;

                if self.check_identifier("relative") {
                    self.advance();
                    state.has_elevation = true;
                    Some(Elevation::Relative)
                } else {
                    let start = self.parse_expression()?;

                    let mut end = None;
                    if self.check(&Token::To) {
                        self.advance(); // consume "to"
                        end = Some(self.parse_expression()?);
                    }

                    state.has_elevation = true;
                    Some(Elevation::Physical { start, end })
                }
            } else {
                return Err(self.error("Expected 'layer' or 'z' after 'on' keyword"));
            }
        } else {
            None
        };

        // v0.1.9: Parse optional position (at [x,y,z])
        // Position is optional when relational constraints are present
        let position = if self.check(&Token::At) {
            self.advance();
            let pos = self.parse_coordinate_optional_z()?;
            state.has_position = true;
            Some(pos)
        } else {
            None
        };

        // v0.1.9: Parse relational constraints (align, above, below, right_of, left_of)
        let mut relational_constraints = smallvec::smallvec![];

        // Parse align constraint: align: <axis> with <target>
        if self.check(&Token::Align) {
            self.advance(); // consume 'align'
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
            relational_constraints.push(RelationalConstraint::Align { axis, target, span });
        }

        // Parse directional constraints: above|below|right_of|left_of <target> [with spacing: <expr>]
        // These can appear in any order after position
        loop {
            if self.check(&Token::Above)
                || self.check(&Token::Below)
                || self.check(&Token::RightOf)
                || self.check(&Token::LeftOf)
            {
                let constraint = if self.check(&Token::Above) {
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
                } else if self.check(&Token::Below) {
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
                } else if self.check(&Token::RightOf) {
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
            } else {
                break;
            }
        }

        // Check for common ordering mistake: rotated before on layer
        if !state.has_elevation && self.check(&Token::Rotated) {
            // User might be trying to put rotated before on layer
            // Look ahead to see if there's an 'on' keyword coming
            let saved_pos = self.current;
            self.advance(); // skip 'rotated'
            if self.parse_rotation().is_ok() && self.check(&Token::On) {
                // Yep, they put rotated before on layer
                self.current = saved_pos; // restore position
                return Err(self.error_context.unexpected_token_error(
                    &Token::Rotated,
                    &self.current_span(),
                    Some(state),
                ));
            }
            self.current = saved_pos; // restore position
        }

        // Parse optional rotation: rotated 45 or rotated -30.5
        let rotation = if self.check(&Token::Rotated) {
            state.has_rotation = true;
            Some(self.parse_rotation()?)
        } else {
            None
        };

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
