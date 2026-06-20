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
            bridges,
            vias: vias_list,
            technology,
            other,
            span: Span::new(start_pos, end_pos),
        })
    }
}
