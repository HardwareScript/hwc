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

        // Parse pin reference
        let pin = self.parse_pin_ref()?;

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
                        _ => return Err(self.error(&format!("Unknown routing mode: '{}'. Expected 'mixed' or 'manual_only'", val))),
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
        let start_pos = self.current_span().start;
        self.expect(&Token::Route)?;

        // Parse from pin reference
        let from = self.parse_pin_ref()?;

        self.expect(&Token::To)?;

        // Parse to pin reference
        let to = self.parse_pin_ref()?;

        let mut width = None;
        let mut strategy = None;
        let mut strategy_params = Vec::new();
        let mut path = None;
        let mut signal_group = None;
        let mut bridge = None;

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

                // Check for keywords or identifiers
                if self.check(&Token::Path) {
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
                    // It's an identifier (width, strategy, or pattern parameter)
                    let key_span = self.current_span();
                    let key = self.expect_identifier_string()?;
                    self.expect(&Token::Colon)?;

                    match key.as_str() {
                        "width" => {
                            width = Some(self.parse_expression()?);
                        }
                        "strategy" => {
                            strategy = Some(self.expect_identifier()?);
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
            strategy,
            strategy_params,
            path,
            signal_group,
            bridge,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse pin reference: `Component.Pin`, `Component[i].Pin`, `Component.Pin[i+1]`, or `Component[i].Pin[j]`
    ///
    /// Supports shorthand for pours: `PourName` (defaults to `PourName.anchor`)
    pub(super) fn parse_pin_ref(&mut self) -> Result<PinReference, ParseError> {
        let start_pos = self.current_span().start;
        let component = self.expect_identifier_string()?;

        // Check for array index on component: Component[i] or Component[i+1]
        let component_index = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            Some(index_expr)
        } else {
            None
        };

        // Shorthand: If no dot follows, default to "anchor" pin (common for pours)
        if !self.check(&Token::Dot) {
            let end_pos = self.previous_span().end;
            return Ok(PinReference {
                component: component.into(),
                component_index,
                pin: "anchor".into(),
                pin_index: None,
                span: Span::new(start_pos, end_pos),
            });
        }

        self.expect(&Token::Dot)?;
        let pin = self.expect_identifier_string()?;

        // Check for array index on pin: Pin[i] or Pin[i-1]
        let pin_index = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            Some(index_expr)
        } else {
            None
        };

        let end_pos = self.previous_span().end;

        Ok(PinReference {
            component: component.into(),
            component_index,
            pin: pin.into(),
            pin_index,
            span: Span::new(start_pos, end_pos),
        })
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
