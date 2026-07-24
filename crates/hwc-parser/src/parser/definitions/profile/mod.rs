//! Profile definition parsing (trace, via, layer, clearance constraints)

mod bridge;
mod constraints;
mod stackup;
mod via;

use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Profile Definition Parsing
    // ========================================================================

    /// Parse profile definition: `define profile "HighVoltage":`
    pub(in super::super) fn parse_profile(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<ProfileDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Profile) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let mut description = None;
        let mut trace = None;
        let mut via = None;
        let mut layer = None;
        let mut clearance = None;
        let mut thermal = None;
        let mut manufacturing = None;
        let mut stackup = None;
        let mut export = None; // v0.1.6: Export & visualization rules
        let mut routing = None; // v0.1.7: Routing constraints (layer directions)
        let mut bridges = Vec::new(); // Phase 1: Bridge rules
        let mut vias_list = Vec::new(); // v0.1.7: Explicit via definitions
        let mut intents = Vec::new(); // CIR Phase 2.2: User-declared routing intents
        let mut technology = None;
        let mut other = rustc_hash::FxHashMap::default(); // v0.1.6: Custom fields

        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                collector.report(
                    self.error("Profile parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Phase 1: Bridge rules
            if self.check(&Token::Bridge) {
                match self.parse_bridge_rule() {
                    Ok(rule) => bridges.push(rule),
                    Err(e) => {
                        collector.report(e);
                        self.sync_to_next_definition();
                    }
                }
                continue;
            }

            // CIR Phase 2.2: User-declared net types (routing intents)
            if self.check(&Token::NetType) {
                match self.parse_profile_net_type(collector) {
                    Some(net_type) => intents.push(net_type),
                    None => {
                        // Error already reported by parse_profile_net_type
                        self.sync_to_next_definition();
                    }
                }
                continue;
            }

            // v0.1.7: Via definitions or constraints
            if self.check_identifier("via") {
                self.advance(); // consume 'via'

                // Check if it's a constraint block: `via:`
                if self.check(&Token::Colon) {
                    self.advance(); // consume ':'
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    via = self.parse_via_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                } else {
                    // It's an explicit via definition: `via Name:`
                    match self.parse_via_definition() {
                        Ok(v) => vias_list.push(v),
                        Err(e) => {
                            collector.report(e);
                            self.sync_to_next_definition();
                        }
                    }
                }
                continue;
            }

            // v0.1.6: Check for property block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "trace" => {
                            self.advance(); // consume 'trace'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            trace = self.parse_trace_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "layer" => {
                            self.advance(); // consume 'layer'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            layer = self.parse_layer_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        "clearance" => {
                            self.advance(); // consume 'clearance'
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            clearance = self.parse_clearance_constraints().ok();
                            if self.check(&Token::Dedent) {
                                self.advance();
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            // Handle other fields as identifiers
            let field_name = match self.expect_identifier() {
                Ok(id) => id,
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
            };

            // Check for deprecated 'intent' keyword before expecting colon
            if field_name.as_str() == "intent" {
                let err = crate::parser::error::ParseError::DeprecatedSyntax {
                    span: crate::parser::error::span_to_source_span(&field_name.span),
                    message:
                        "The 'intent' keyword has been renamed to 'net_type'. Use: net_type Signal:"
                            .into(),
                };
                collector.report(err);
                self.sync_to_next_definition();
                continue;
            }

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            match field_name.as_str() {
                "description" => {
                    description = self.expect_string().ok();
                    self.skip_whitespace();
                }
                "thermal" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    thermal = self.parse_thermal_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "manufacturing" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    manufacturing = self.parse_manufacturing_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "stackup" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    stackup = self.parse_stackup_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "export" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    export = self.parse_export_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "routing" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    routing = self.parse_routing_constraints().ok();
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                "technology" => {
                    technology = self.expect_string().ok();
                    self.skip_whitespace();
                }
                _ => {
                    // v0.1.6: Accept unknown fields and store in 'other' HashMap
                    if let Some(current) = self.current() {
                        if matches!(current.token, Token::String(_)) {
                            if let Ok(value) = self.expect_string() {
                                other.insert(field_name.name, value);
                            }
                            self.skip_whitespace();
                            continue;
                        }
                    }

                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }

                    // Skip the entire block
                    let mut depth = 1;
                    while depth > 0 && !self.is_at_end() {
                        if self.check(&Token::Indent) {
                            depth += 1;
                            self.advance();
                        } else if self.check(&Token::Dedent) {
                            depth -= 1;
                            self.advance();
                        } else {
                            self.advance();
                        }
                    }
                }
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                self.advance();
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(ProfileDefinition {
            name,
            is_exported,
            description: description.map(|s: String| s.into()),
            trace,
            via,
            layer,
            clearance,
            thermal,
            manufacturing,
            stackup,
            export,
            routing,
            intents,
            bridges,
            vias: vias_list,
            technology,
            other,
            span: Span::new(start_pos, end_pos),
        })
    }

    // ========================================================================
    // Profile Net Type Parsing (CIR Phase 2.2)
    // ========================================================================

    /// Parse a net type declaration block: `net_type <Name>:`
    ///
    /// Syntax:
    /// ```hw
    /// net_type Clock:
    ///     routing_style: straight
    ///     cost_weights:
    ///         base: 10
    ///         via_penalty: 500
    /// ```
    fn parse_profile_net_type(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<ProfileIntent> {
        let start_pos = self.current_span().start;

        // Consume 'net_type' keyword
        if let Err(e) = self.expect(&Token::NetType) {
            collector.report(e);
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }

        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            return None;
        }

        let mut routing_style = None;
        let mut cost_weights = None;
        let mut escape_stub = None; // v0.1.9: Declarative Escape Policies

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = match self.expect_identifier() {
                Ok(id) => id,
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
            };

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            match field_name.as_str() {
                "routing_style" => {
                    routing_style = self.expect_identifier().ok();
                    self.skip_whitespace();
                }
                "escape_stub" => {
                    escape_stub = self.parse_measurement().ok();
                    self.skip_whitespace();
                }
                "cost_weights" => {
                    if let Err(e) = self.expect(&Token::Newline) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    if let Err(e) = self.expect(&Token::Indent) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                    cost_weights = self.parse_cost_weights(collector);
                    if self.check(&Token::Dedent) {
                        self.advance();
                    }
                }
                _ => {
                    // Skip unknown fields
                    self.skip_whitespace();
                }
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Some(ProfileIntent {
            name,
            routing_style,
            cost_weights,
            escape_stub, // v0.1.9
            span: Span::new(start_pos, self.previous_span().end),
        })
    }

    /// Parse cost weights block inside a net_type declaration.
    ///
    /// Syntax:
    /// ```hw
    /// cost_weights:
    ///     base: 10
    ///     via_penalty: 500
    ///     direction_penalty: 20
    ///     tight_clearance_penalty: 5
    ///     crosstalk_penalty: 10
    ///     impedance_penalty: 3
    ///     reference_void_penalty: 10000000
    /// ```
    fn parse_cost_weights(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<CostWeights> {
        let start_pos = self.current_span().start;

        let mut base = None;
        let mut via_penalty = None;
        let mut direction_penalty = None;
        let mut tight_clearance_penalty = None;
        let mut crosstalk_penalty = None;
        let mut impedance_penalty = None;
        let mut reference_void_penalty = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_name = match self.expect_identifier() {
                Ok(id) => id,
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
            };

            if let Err(e) = self.expect(&Token::Colon) {
                collector.report(e);
                self.sync_to_next_definition();
                continue;
            }

            match field_name.as_str() {
                "base" => {
                    base = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "via_penalty" => {
                    via_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "direction_penalty" => {
                    direction_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "tight_clearance_penalty" => {
                    tight_clearance_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "crosstalk_penalty" => {
                    crosstalk_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "impedance_penalty" => {
                    impedance_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                "reference_void_penalty" => {
                    reference_void_penalty = self.expect_integer().ok().map(|v| v as i64);
                    self.skip_whitespace();
                }
                _ => {
                    self.skip_whitespace();
                }
            }
        }

        let end_pos = self.previous_span().end;

        Some(CostWeights {
            base,
            via_penalty,
            direction_penalty,
            tight_clearance_penalty,
            crosstalk_penalty,
            impedance_penalty,
            reference_void_penalty,
            span: Span::new(start_pos, end_pos),
        })
    }
}
