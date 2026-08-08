//! Main component definition parsing and error recovery

use super::super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};
use smallvec::SmallVec;

impl<'ast> super::super::super::Parser<'ast> {
    /// Parse component definition: `define component "Resistor_0805" (val: Measurement):`
    pub(in super::super::super) fn parse_component_def(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<ComponentDefinition> {
        let start_pos = self.current_span().start;

        // eprintln!("[DEBUG] Parsing component definition...");

        if let Err(e) = self.expect(&Token::Component) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(id) => {
                // eprintln!("[DEBUG] Component name: {}", id.name);
                id
            }
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        // Parse optional parameters: (val: Measurement, tol: Measurement)
        let parameters: SmallVec<[ComponentParameter; 4]> = if self.check(&Token::OpenParen) {
            self.advance();
            match self.parse_component_parameters() {
                Ok(params) => {
                    if let Err(e) = self.expect(&Token::CloseParen) {
                        collector.report(e);
                        self.sync_to_next_definition();
                        return None;
                    }
                    params
                }
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    return None;
                }
            }
        } else {
            SmallVec::new()
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

        let mut metadata = None;
        let mut pins = SmallVec::new();
        let mut layout = None;
        let mut electrical = None;
        let mut render = None;

        // Parse component blocks
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            // Check if we should stop (hit error limit)
            if collector.should_stop() {
                // eprintln!("[DEBUG] Hit error limit, stopping component parse");
                break;
            }

            // CRITICAL SAFETY: Track current position to detect infinite loops
            let position_before = self.current;

            // eprintln!("[DEBUG] Component block loop iteration, current token: {:?}",
            //     self.current().map(|s| &s.token)
            // );

            // Skip blank lines and comments
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // v0.1.6: If we encounter any type keyword, we've exited the component block
            if self.check(&Token::Component)
                || self.check(&Token::Space)
                || self.check(&Token::Material)
                || self.check(&Token::Profile)
                || self.check(&Token::Module)
                || self.check(&Token::Mechanical)
                || self.check(&Token::Interface)
                || self.check(&Token::Test)
                || self.check(&Token::Unit)
                || self.check(&Token::SignalGroup)
                || self.check(&Token::Logic)
                || self.check(&Token::Enum)
                || self.check(&Token::Struct)
            {
                // eprintln!("[DEBUG] Breaking out of component loop, found top-level keyword");
                break;
            }

            // eprintln!("[DEBUG] Past break check, continuing to parse component blocks");

            // v0.1.6: Check for property block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "metadata" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }

                            // Proper panic mode recovery: sub-parser returns Result
                            match self.parse_component_metadata() {
                                Ok(m) => {
                                    metadata = Some(m);
                                    // Consume the dedent after metadata block
                                    if self.check(&Token::Dedent) {
                                        self.advance();
                                    }
                                }
                                Err(e) => {
                                    collector.report(e);
                                    self.sync_to_next_component_block();
                                }
                            }
                            continue;
                        }
                        "pins" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                // Skip to next line and continue parsing component
                                while !self.is_at_end() && !self.check(&Token::Newline) {
                                    self.advance();
                                }
                                if self.check(&Token::Newline) {
                                    self.advance();
                                }
                                continue;
                            }
                            match self.parse_pins_block() {
                                Ok(p) => pins = p,
                                Err(e) => {
                                    collector.report(e);
                                    self.sync_to_next_component_block();
                                }
                            }
                            // Note: parse_pins_block handles dedent internally for block format
                            // For inline format, there's no dedent to consume
                            continue;
                        }
                        "layout" => {
                            // eprintln!("[DEBUG] Matched layout block");
                            self.advance();
                            // eprintln!("[DEBUG] After advance, current token: {:?}", self.current().map(|s| &s.token));
                            if let Err(e) = self.expect(&Token::Colon) {
                                // eprintln!("[DEBUG] Failed to expect colon after layout: {:?}", e);
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                // eprintln!("[DEBUG] Failed to expect newline after layout colon: {:?}", e);
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                // eprintln!("[DEBUG] Failed to expect indent after layout newline: {:?}", e);
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }

                            // Proper panic mode recovery: sub-parser returns Result
                            // eprintln!("[DEBUG] About to parse layout block");
                            // eprintln!("[DEBUG] Current position in token stream: {}/{}", self.current, self.tokens.len());
                            match self.parse_layout_block() {
                                Ok(l) => {
                                    // eprintln!("[DEBUG] Layout block parsed successfully");
                                    // eprintln!("[DEBUG] After layout parse, position: {}/{}", self.current, self.tokens.len());
                                    layout = Some(l);
                                    // Consume the dedent after layout block
                                    if self.check(&Token::Dedent) {
                                        // eprintln!("[DEBUG] Consuming dedent after layout");
                                        self.advance();
                                        // eprintln!("[DEBUG] After dedent, position: {}/{}", self.current, self.tokens.len());
                                    }
                                }
                                Err(e) => {
                                    // eprintln!("[DEBUG] Layout block parse failed: {:?}", e);
                                    collector.report(e);
                                    self.sync_to_next_component_block();
                                }
                            }
                            // eprintln!("[DEBUG] After layout block, continuing");
                            // eprintln!("[DEBUG] Current token after layout: {:?}", self.current().map(|s| &s.token));
                            // eprintln!("[DEBUG] Is at end: {}, Is dedent: {}", self.is_at_end(), self.check(&Token::Dedent));
                            continue;
                        }
                        "electrical" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }

                            // Proper panic mode recovery: sub-parser returns Result
                            match self.parse_electrical_block() {
                                Ok(e_block) => {
                                    electrical = Some(e_block);
                                    // Consume the dedent after electrical block
                                    if self.check(&Token::Dedent) {
                                        self.advance();
                                    }
                                }
                                Err(e) => {
                                    collector.report(e);
                                    self.sync_to_next_component_block();
                                }
                            }
                            continue;
                        }
                        "render" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Newline) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }
                            if let Err(e) = self.expect(&Token::Indent) {
                                collector.report(e);
                                self.sync_to_next_component_block();
                                continue;
                            }

                            // Proper panic mode recovery: sub-parser returns Result
                            match self.parse_render_block() {
                                Ok(r) => {
                                    render = Some(r);
                                    // Consume the dedent after render block
                                    if self.check(&Token::Dedent) {
                                        self.advance();
                                    }
                                }
                                Err(e) => {
                                    collector.report(e);
                                    self.sync_to_next_component_block();
                                }
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            if self.check(&Token::Newline) {
                self.advance();
            } else if !self.check(&Token::Dedent) {
                // CRITICAL: If we didn't match anything and we're not at dedent,
                // we MUST advance to prevent infinite loop
                let err = self.error(&format!(
                    "Expected component block (metadata, pins, layout, electrical, render), found {:?}",
                    self.current()
                ));
                collector.report(err);
                // Advance to prevent infinite loop
                self.advance();
            }

            // CRITICAL SAFETY: Ensure we made progress to prevent infinite loops
            if self.current == position_before {
                // We didn't advance at all - this is a bug, force advance
                eprintln!(
                    "[PARSER BUG] Infinite loop detected in component parser, forcing advance"
                );
                self.advance();
            }
        }

        // eprintln!("[DEBUG] Exited component block loop");

        // Consume the dedent that ends the component definition
        if self.check(&Token::Dedent) {
            // eprintln!("[DEBUG] Consuming dedent at end of component");
            self.advance();
        }

        // eprintln!("[DEBUG] Component definition complete: {}", name.name);

        let end_pos = self.previous_span().end;

        Some(ComponentDefinition {
            name,
            is_exported,
            parameters,
            metadata,
            pins,
            layout,
            electrical,
            render,
            implements: SmallVec::new(), // TODO: Parse interface implementations
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Synchronize to the next component block after a parse error.
    ///
    /// This is the CRITICAL function that prevents infinite loops!
    /// It GUARANTEES forward progress by:
    /// 1. Always advancing at least one token
    /// 2. Consuming tokens until we find a safe recovery point
    ///
    /// Safe recovery points are:
    /// - Start of a component block (metadata:, pins:, layout:, etc.)
    /// - Dedent (end of component.into())
    /// - Top-level definition keyword (component.into(), space, etc.)
    pub(super) fn sync_to_next_component_block(&mut self) {
        // CRITICAL: Always advance at least once to guarantee progress
        if !self.is_at_end() {
            self.advance();
        }

        // Now consume tokens until we find a safe place to resume
        while !self.is_at_end() {
            if self.check(&Token::Dedent) {
                break; // Reached end of component
            }

            if let Some(spanned) = self.current() {
                match &spanned.token {
                    // Component block keywords - safe to resume here
                    Token::Identifier(name)
                        if matches!(
                            name.as_str(),
                            "metadata" | "pins" | "layout" | "electrical" | "render"
                        ) =>
                    {
                        break
                    }

                    // Top-level keywords - we've exited the component
                    Token::Component
                    | Token::Space
                    | Token::Material
                    | Token::Profile
                    | Token::Module
                    | Token::Mechanical
                    | Token::Interface
                    | Token::Test
                    | Token::Unit
                    | Token::SignalGroup
                    | Token::Logic
                    | Token::Enum
                    | Token::Struct => break,

                    _ => self.advance(), // Keep consuming bad tokens
                }
            } else {
                break;
            }
        }
    }

    /// Parse component parameters: `val: Measurement, tol: Measurement`
    fn parse_component_parameters(
        &mut self,
    ) -> Result<SmallVec<[crate::ast::ComponentParameter; 4]>, ParseError> {
        let mut parameters = SmallVec::new();

        loop {
            if self.check(&Token::CloseParen) {
                break;
            }

            // v0.1.6: Parameter names are now regular identifiers
            let param_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;
            let param_type = self.expect_identifier_string()?;

            parameters.push(crate::ast::ComponentParameter {
                name: param_name.into(),
                param_type: param_type.into(),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(parameters)
    }
}
