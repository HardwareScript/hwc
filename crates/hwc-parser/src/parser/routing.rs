//! Route and expose statement parsing

use super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::Parser {
    // ========================================================================
    // Routing Parsing
    // ========================================================================

    /// Parse expose statement: `expose Pin as Alias`
    pub(super) fn parse_expose(&mut self) -> Result<Expose, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Expose)?;

        // Parse endpoint reference
        let pin = self.parse_route_endpoint()?;

        self.expect(&Token::As)?;

        // Parse alias
        let alias = self.expect_identifier_string()?;

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(Expose {
            pin,
            alias: alias.into(),
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse substrate placement: `add substrate(material) spanning [Z,X,Y] to [Z,X,Y]`
    ///
    /// v0.1.7 Phase 2.2: Supports optional `cutouts:` property block.
    /// ```hardware
    /// add substrate(Silicon_N) spanning [0,0,0] to [10mm,10mm,500um]:
    ///     cutouts:
    ///         - [x:2mm, y:2mm] to [x:3mm, y:3mm]
    /// ```
    pub(super) fn parse_substrate(&mut self) -> Result<SubstratePlacement, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Add)?;
        self.expect(&Token::Substrate)?;
        self.expect(&Token::OpenParen)?;

        let material = self.expect_identifier_string()?;

        self.expect(&Token::CloseParen)?;
        self.expect(&Token::Spanning)?;

        let from = self.parse_coordinate()?;

        self.expect(&Token::To)?;

        let to = self.parse_coordinate()?;

        // v0.1.7 Phase 2.2: Parse optional properties block (cutouts)
        let cutouts = if self.check(&Token::Colon) {
            self.advance(); // consume ':'
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            let mut cutouts = Vec::new();

            while !self.is_at_end() && !self.check(&Token::Dedent) {
                self.skip_whitespace();
                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                let field_name = self.expect_identifier_or_keyword_string()?;
                self.expect(&Token::Colon)?;

                match field_name.as_str() {
                    "cutouts" => {
                        self.expect(&Token::Newline)?;
                        self.expect(&Token::Indent)?;

                        while !self.is_at_end() && !self.check(&Token::Dedent) {
                            self.skip_whitespace();
                            if self.check(&Token::Dedent) || self.is_at_end() {
                                break;
                            }

                            // Each cutout: `- [x:..., y:...] to [x:..., y:...]`
                            self.expect(&Token::Hyphen)?;
                            let cutout_from = self.parse_coordinate()?;
                            self.expect(&Token::To)?;
                            let cutout_to = self.parse_coordinate()?;
                            cutouts.push(CoordinatePair {
                                from: cutout_from,
                                to: cutout_to,
                            });

                            self.skip_whitespace();
                        }

                        self.expect(&Token::Dedent)?;
                    }
                    _ => {
                        return Err(self.error(&format!(
                            "Unknown substrate property: '{}'. Expected 'cutouts'",
                            field_name
                        )));
                    }
                }

                self.skip_whitespace();
            }

            self.expect(&Token::Dedent)?;
            cutouts
        } else {
            self.skip_whitespace();
            Vec::new() // No cutouts
        };

        let end_pos = self.previous_span().end;

        Ok(SubstratePlacement {
            material: material.into(),
            from,
            to,
            cutouts,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse global routing configuration: `routing:` block
    pub(super) fn parse_routing_config(&mut self) -> Result<RoutingConfig, ParseError> {
        let start_pos = self.current_span().start;
        self.expect_identifier_named("routing")?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut mode = RoutingMode::Mixed;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let key = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "mode" => {
                    let val = self.expect_identifier_string()?;
                    match val.as_str() {
                        "mixed" => mode = RoutingMode::Mixed,
                        "manual_only" => mode = RoutingMode::ManualOnly,
                        _ => {
                            return Err(self.error(&format!(
                                "Unknown routing mode: '{}'. Expected 'mixed' or 'manual_only'",
                                val
                            )))
                        }
                    }
                }
                _ => return Err(self.error(&format!("Unknown routing property: '{}'", key))),
            }
            self.skip_whitespace();
        }

        self.expect(&Token::Dedent)?;
        let end_pos = self.previous_span().end;

        Ok(RoutingConfig {
            mode,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse route: `route From.Pin to To.Pin` (auto-routing)
    /// or `route From.Pin to To.Pin:` with `path:` and optional `strategy:` (v0.1.7)
    pub(super) fn parse_route(&mut self) -> Result<Route, ParseError> {
        // Enter route context for better error messages
        self.error_context
            .enter_context(crate::parser::ParsingContext::RouteStatement);

        // Track parsing state
        let mut state = crate::parser::RouteParseState::default();

        let result = self.parse_route_impl(&mut state);

        // Exit context
        self.error_context.exit_context();

        result
    }

    /// Internal route parsing with state tracking
    fn parse_route_impl(
        &mut self,
        state: &mut crate::parser::RouteParseState,
    ) -> Result<Route, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Route)?;

        // Parse from endpoint
        let from = self.parse_route_endpoint()?;
        state.has_from = true;

        self.expect(&Token::To)?;

        // Parse to endpoint
        let to = self.parse_route_endpoint()?;
        state.has_to = true;

        let mut width = None;
        let mut layer = None;
        let mut strategy = None;
        let mut pattern = None;
        let mut strategy_params = Vec::new();
        let mut path = None;
        let mut signal_group = None;
        let mut bridge = None;
        let mut exit_escape = None;
        let mut enter_escape = None;
        let mut current_limit_ac = None;

        // Check if this has a properties block (starts with colon)
        if self.check(&Token::Colon) {
            self.advance(); // consume ':'
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                // v0.1.7: Check for exit/enter escape keywords
                if self.check(&Token::Exit) {
                    self.advance(); // consume 'exit'
                    self.expect(&Token::Colon)?;
                    exit_escape = Some(self.parse_route_escape()?);
                } else if self.check(&Token::Enter) {
                    self.advance(); // consume 'enter'
                    self.expect(&Token::Colon)?;
                    enter_escape = Some(self.parse_route_escape()?);
                } else if self.check(&Token::Path) {
                    self.advance(); // consume 'path'
                    self.expect(&Token::Colon)?;

                    // v0.1.7: Support bracketed path: [ [x,y,z], [x,y,z] ]
                    if self.check(&Token::OpenBracket) {
                        self.advance(); // consume '['
                        let mut waypoints = Vec::new();
                        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                            waypoints.push(self.parse_coordinate()?);
                            if self.check(&Token::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(&Token::CloseBracket)?;
                        path = Some(waypoints);
                    } else {
                        // Legacy: Bulleted list
                        self.expect(&Token::Newline)?;
                        if self.check(&Token::Indent) {
                            self.advance();
                            path = Some(self.parse_waypoints()?);
                            self.expect(&Token::Dedent)?;
                        }
                    }
                } else if self.check(&Token::SignalGroup) {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    signal_group = Some(self.expect_string()?.into());
                } else if self.check(&Token::Bridge) {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    bridge = Some(self.expect_identifier_string()?.into());
                } else {
                    // It's an identifier (width, strategy, current_limit, or pattern parameter)
                    let key_span = self.current_span();
                    let key = self.expect_identifier_string()?;
                    self.expect(&Token::Colon)?;

                    match key.as_str() {
                        "width" => {
                            width = Some(self.parse_expression()?);
                            state.has_width = true;
                        }
                        "layer" => {
                            layer = Some(self.expect_identifier()?);
                            state.has_layer = true;
                        }
                        "strategy" => {
                            strategy = Some(self.expect_identifier()?);
                            state.has_strategy = true;
                        }
                        "pattern" => {
                            pattern = Some(self.parse_pattern_instantiation()?);
                        }
                        "current_limit" | "current_limit_ac" => {
                            state.has_current_limit = true;
                            // Parse: current_limit: [rms: <Value>, peak: <Value>]
                            // or: current_limit_ac: { rms: <Value>, peak: <Value> }
                            // Or backward compat: current_limit: <Value>
                            if self.check(&Token::OpenBracket) || self.check(&Token::OpenBrace) {
                                let is_brace = self.check(&Token::OpenBrace);
                                self.advance(); // consume '[' or '{'
                                let mut rms = None;
                                let mut peak = None;

                                let close_token = if is_brace {
                                    Token::CloseBrace
                                } else {
                                    Token::CloseBracket
                                };

                                while !self.check(&close_token) && !self.is_at_end() {
                                    if self.check(&Token::Newline) {
                                        self.advance();
                                        continue;
                                    }
                                    let key_name = self.expect_identifier_string()?;
                                    self.expect(&Token::Colon)?;
                                    let val = self.parse_expression()?;

                                    match key_name.as_str() {
                                        "rms" => rms = Some(val),
                                        "peak" => peak = Some(val),
                                        _ => {
                                            return Err(self.error(&format!(
                                                "Unknown current_limit field: '{}'. Expected 'rms' or 'peak'",
                                                key_name
                                            )));
                                        }
                                    }

                                    if self.check(&Token::Comma) {
                                        self.advance();
                                    }
                                }
                                self.expect(&close_token)?;

                                let rms_expr = rms.ok_or_else(|| {
                                    self.error("current_limit missing 'rms' field")
                                })?;
                                let peak_expr = peak.ok_or_else(|| {
                                    self.error("current_limit missing 'peak' field")
                                })?;

                                let cl_span = Span::new(key_span.start, self.previous_span().end);
                                current_limit_ac = Some(CurrentLimitAc {
                                    rms: rms_expr,
                                    peak: peak_expr,
                                    span: cl_span,
                                });
                            } else {
                                // Backward compat: single value treated as DC (both rms and peak)
                                let val = self.parse_expression()?;
                                let cl_span = Span::new(key_span.start, self.previous_span().end);
                                current_limit_ac = Some(CurrentLimitAc {
                                    rms: val.clone(),
                                    peak: val,
                                    span: cl_span,
                                });
                            }
                        }
                        _ => {
                            // Pattern parameter
                            let value = self.parse_expression()?;
                            strategy_params.push((Identifier::new(key.into(), key_span), value));
                        }
                    }
                }
                self.skip_whitespace();
            }

            self.expect(&Token::Dedent)?;
        } else {
            self.skip_whitespace();
        }

        let end_pos = self.previous_span().end;

        Ok(Route {
            from,
            to,
            width,
            layer,
            strategy,
            pattern,
            strategy_params,
            path,
            signal_group,
            bridge,
            exit_escape,
            enter_escape,
            current_limit_ac,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse route net policy: `route net: NetName:` with optional `on layer:` clause
    ///
    /// v0.1.8: Prescriptive net-scoped route policy for auto routing.
    /// Example:
    /// ```hardware
    /// route net: ALL_PADS:
    ///     pattern: Zigzag(gap: 0.5mm)
    ///
    /// route net: DDR5_BUS on layer: top:
    ///     pattern: Trombone(gap: 0.3mm, amp: 2.5mm)
    /// ```
    pub(super) fn parse_route_net_policy(&mut self) -> Result<RouteNetPolicy, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Route)?;
        self.expect_identifier_named("net")?;
        self.expect(&Token::Colon)?;

        let net_id = self.expect_identifier()?;

        // Check for optional `on layer: <Layer>` clause
        let target_layer = if self.check(&Token::On) {
            self.advance(); // consume 'on'
            self.expect_identifier_named("layer")?;
            self.expect(&Token::Colon)?;
            Some(self.expect_identifier()?)
        } else {
            None
        };

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut pattern = None;
        let mut strategy = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "pattern" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            pattern = Some(self.parse_pattern_instantiation()?);
                            self.skip_whitespace();
                            continue;
                        }
                        "strategy" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            strategy = Some(self.expect_identifier()?);
                            self.skip_whitespace();
                            continue;
                        }
                        other => {
                            return Err(self.error(&format!(
                                "Unknown route net policy field: '{}'. Expected: pattern, strategy",
                                other
                            )));
                        }
                    }
                }
            }

            break;
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(RouteNetPolicy {
            net_id,
            target_layer,
            pattern,
            strategy,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse route escape specification: `East at top`, `East at 80%`, `East at +150um`
    fn parse_route_escape(&mut self) -> Result<RouteEscape, ParseError> {
        let start_pos = self.current_span().start;

        // Parse cardinal direction
        let port = if let Some(current) = self.current() {
            let dir = match &current.token {
                Token::TopLeft => {
                    self.advance();
                    CardinalDirection::North
                }
                Token::BottomLeft => {
                    self.advance();
                    CardinalDirection::South
                }
                Token::TopRight => {
                    self.advance();
                    CardinalDirection::East
                }
                Token::BottomRight => {
                    self.advance();
                    CardinalDirection::West
                }
                Token::Identifier(s) => {
                    let s = s.to_lowercase();
                    self.advance();
                    match s.as_str() {
                        "north" | "n" | "top" => CardinalDirection::North,
                        "south" | "s" | "bottom" => CardinalDirection::South,
                        "east" | "e" | "right" => CardinalDirection::East,
                        "west" | "w" | "left" => CardinalDirection::West,
                        _ => {
                            return Err(self.error(&format!(
                            "Expected cardinal direction (North, South, East, West), found '{}'",
                            s
                        )))
                        }
                    }
                }
                _ => {
                    return Err(self.error("Expected cardinal direction (North, South, East, West)"))
                }
            };
            dir
        } else {
            return Err(self.error("Expected cardinal direction"));
        };

        // Parse optional offset (at ...)
        let offset = if self.check(&Token::At) {
            self.advance(); // consume 'at'
            Some(self.parse_edge_offset()?)
        } else {
            None
        };

        let end_pos = self.previous_span().end;

        Ok(RouteEscape {
            port,
            offset,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse edge offset: "top", "bottom", "center", "80%", "+150um", "-50um"
    fn parse_edge_offset(&mut self) -> Result<EdgeOffsetSpec, ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Identifier(s) => {
                    let s = s.to_lowercase();
                    self.advance();
                    match s.as_str() {
                        "top" | "max" | "high" | "upper" => {
                            Ok(EdgeOffsetSpec::Named(NamedPosition::Top))
                        }
                        "bottom" | "min" | "low" | "lower" => {
                            Ok(EdgeOffsetSpec::Named(NamedPosition::Bottom))
                        }
                        "center" | "centre" | "mid" | "middle" => {
                            Ok(EdgeOffsetSpec::Named(NamedPosition::Center))
                        }
                        _ => {
                            // Check if it's a percentage like "80%"
                            if s.ends_with('%') {
                                let pct_str = &s[..s.len() - 1];
                                if let Ok(val) = pct_str.parse::<f64>() {
                                    return Ok(EdgeOffsetSpec::Percentage(val / 100.0));
                                }
                            }
                            // Check if it's a measurement like "+150um" or "-50um"
                            if s.ends_with("um") {
                                let val_str = &s[..s.len() - 2];
                                if let Ok(val) = val_str.parse::<f64>() {
                                    return Ok(EdgeOffsetSpec::Measurement((val * 1000.0) as i64));
                                }
                            }
                            Err(self.error(&format!(
                                "Expected edge offset (top, bottom, center, N%, +/-Numum), found '{}'",
                                s
                            )))
                        }
                    }
                }
                Token::Float(n) => {
                    let val = *n;
                    self.advance();
                    // Plain number as percentage
                    if (0.0..=1.0).contains(&val) {
                        Ok(EdgeOffsetSpec::Percentage(val))
                    } else {
                        Err(self.error(&format!(
                            "Percentage must be between 0.0 and 1.0, found {}",
                            val
                        )))
                    }
                }
                _ => Err(self.error("Expected edge offset (top, bottom, center, N%, +/-Numum)")),
            }
        } else {
            Err(self.error("Expected edge offset"))
        }
    }

    /// Parse waypoints: list of coordinates in path block
    /// Empty path block is allowed for automatic routing
    pub(super) fn parse_waypoints(&mut self) -> Result<Vec<Coordinate>, ParseError> {
        let mut waypoints = Vec::new();

        // Parse waypoints (each line starts with '-')
        while self.check(&Token::Hyphen) {
            self.advance(); // consume '-'
            waypoints.push(self.parse_coordinate()?);
            self.skip_whitespace();
        }

        // Empty waypoints vector is allowed - indicates automatic routing
        Ok(waypoints)
    }
}
