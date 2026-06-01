//! Core space definition parsing

use crate::ast::*;
use crate::lexer::{Span, Token};

impl crate::parser::Parser {
    /// Parse space definition: `define space "Name":`
    pub(in crate::parser) fn parse_space(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<SpaceDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Space) {
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

        // Check for optional 'implements ModuleName' clause (Phase 3: Progressive Alignment)
        // Pattern: space CMOS_Inverter implements Inverter_Logic:
        let implements_module = if self.check(&Token::Implements) {
            self.advance(); // consume 'implements'
            match self.expect_identifier_string() {
                Ok(module_name) => Some(module_name),
                Err(e) => {
                    collector.report(e);
                    None
                }
            }
        } else {
            None
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

        let mut dimensions: Option<Dimensions> = None;
        let mut grid: Option<Grid> = None;
        let mut origin: Option<OriginPoint> = None;
        let mut profile: Option<Identifier> = None;
        let mut mechanical: Option<Identifier> = None;
        let mut substrate = None;
        let mut render = None;
        let mut routing_config = None;

        // v0.1.7 CRITICAL FIX: Unified statement stream (preserves textual order)
        let mut statements = Vec::new();

        // REMOVED (pre-release): The deprecated parallel vecs for components/pours/... were eliminated here along with AST fields.
        // See ast/space.rs:77 for full rationale on avoiding dual-storage migration patterns.
        let mut layouts = Vec::new();
        let mut routes = Vec::new();
        let mut exposes = Vec::new();
        let mut nets = Vec::new();

        // Parse space body
        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Space parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Space parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            // Collect doc comments (for future use)
            let _doc_comments = self.collect_doc_comments();

            // Skip remaining whitespace
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Unified control flow - single if-else chain
            if self.check(&Token::Dimensions) {
                dimensions = self.parse_dimensions().ok();
            } else if self.check(&Token::Grid) {
                grid = self.parse_grid().ok();
            } else if self.check(&Token::Origin) {
                origin = self.parse_origin().ok();
            } else if self.check(&Token::Add) {
                // Check if it's a substrate, pour, polygon, contact, or component
                let next_pos = self.current + 1;
                if let Some(next_token) = self.tokens.get(next_pos) {
                    match &next_token.token {
                        Token::Substrate => {
                            match self.parse_substrate() {
                                Ok(sub) => {
                                    statements.push(SpaceTopLevelStatement::Substrate(sub.clone()));
                                    if substrate.is_none() {
                                        substrate = Some(sub); // Legacy field: store first substrate
                                    }
                                }
                                Err(err) => {
                                    collector.report(err);
                                    self.sync_to_next_definition();
                                    continue;
                                }
                            }
                        }
                        Token::Pour => match self.parse_pour() {
                            Ok(pour) => {
                                statements.push(SpaceTopLevelStatement::Pour(pour));
                            }
                            Err(err) => {
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        },
                        Token::Polygon => {
                            if let Ok(polygon) = self.parse_polygon() {
                                statements.push(SpaceTopLevelStatement::Polygon(polygon));
                            }
                        }
                        Token::Contact => match self.parse_contact() {
                            Ok(contact) => {
                                statements.push(SpaceTopLevelStatement::Contact(contact));
                            }
                            Err(err) => {
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        },
                        _ => match self.parse_component_placement() {
                            Ok(comp) => {
                                statements.push(SpaceTopLevelStatement::Component(comp));
                            }
                            Err(err) => {
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        },
                    }
                    } else {
                        match self.parse_component_placement() {
                            Ok(comp) => {
                                statements.push(SpaceTopLevelStatement::Component(comp));
                            }
                            Err(err) => {
                                collector.report(err);
                                self.sync_to_next_definition();
                                continue;
                            }
                        }
                    }
            } else if self.check(&Token::For) {
                // Sprint 3.4: Parse for loops in space blocks
                match self.parse_space_for_loop() {
                    Ok(for_loop) => {
                        statements.push(SpaceTopLevelStatement::ForLoop(for_loop));
                    }
                    Err(err) => {
                        collector.report(err);
                        self.sync_to_next_definition();
                        continue;
                    }
                }
            } else if self.check(&Token::Route) {
                if let Ok(route) = self.parse_route() {
                    statements.push(SpaceTopLevelStatement::Route(route.clone()));
                    routes.push(route); // Note: routes still duplicated in top-level vec (not yet cleaned)
                }
            } else if self.check(&Token::Expose) {
                if let Ok(expose) = self.parse_expose() {
                    statements.push(SpaceTopLevelStatement::Expose(expose.clone()));
                    exposes.push(expose); // Note: exposes still duplicated in top-level vec (not yet cleaned)
                }
            } else if self.check(&Token::Profile) {
                self.advance(); // consume 'profile'
                if let Err(e) = self.expect(&Token::Colon) {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
                profile = self.expect_namespaced_identifier().ok();
                self.skip_whitespace();
            } else if self.check(&Token::Mechanical) {
                self.advance(); // consume 'mechanical'
                if let Err(e) = self.expect(&Token::Colon) {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
                mechanical = self.expect_namespaced_identifier().ok();
                self.skip_whitespace();
            } else if self.check(&Token::Identifier("nets".into())) {
                // Parse nets block for net classifications (v0.1.6)
                match self.parse_nets_block() {
                    Ok(net_decls) => {
                        nets.extend(net_decls);
                    }
                    Err(e) => {
                        collector.report(e);
                        self.sync_to_next_definition();
                        continue;
                    }
                }
            } else if self.check(&Token::Identifier("routing".into())) {
                // Parse global routing policy (v0.1.7)
                routing_config = self.parse_routing_config().ok();
            } else if self.check(&Token::Identifier("render".into())) {
                self.advance(); // consume 'render'
                if let Err(e) = self.expect(&Token::Colon) {
                    collector.report(e);
                    self.sync_to_next_definition();
                    continue;
                }
                render = self.parse_render_block().ok();
            } else if self.check(&Token::Newline) {
                self.advance();
            } else if let Some(current) = self.current() {
                // v0.1.6: Check for 'layout' identifier or doc comments
                if let Token::Identifier(name) = &current.token {
                    if name == "layout" {
                        if let Ok(layout) = self.parse_module_layout_block() {
                            layouts.push(layout);
                        }
                        continue;
                    } else {
                        // Unknown identifier
                        let err = self.error(&format!(
                            "Unknown space field: '{}'. Expected 'dimensions', 'grid', 'origin', 'profile', 'mechanical', 'add', 'route', or 'expose'",
                            name
                        ));
                        collector.report(err);
                        self.sync_to_next_definition();
                        continue;
                    }
                } else {
                    // Unexpected token
                    let err = self.error(&format!(
                        "Unexpected token in space definition: {}",
                        current.token
                    ));
                    collector.report(err);
                    self.sync_to_next_definition();
                    break;
                }
            } else {
                break;
            }

            // CRITICAL SAFETY: Ensure we made progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Space parser didn't advance, forcing progress");
                self.advance();
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(SpaceDefinition {
            name,
            implements_module: implements_module.map(|s: String| s.into()),
            dimensions,
            grid,
            origin,
            profile,
            mechanical,
            substrate,
            render,
            routing_config,
            statements, // v0.1.7: Unified statement stream (sole canonical representation post-cleanup)
            layouts,
            routes,
            exposes,
            nets,
            span: Span::new(start_pos, end_pos),
        })
    }
}
