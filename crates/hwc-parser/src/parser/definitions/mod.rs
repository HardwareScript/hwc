//! Definition parsing module
//!
//! This module is organized into logical submodules by definition type:
//! - `bridge`: Bridge definitions (v0.2.0: First-class material transitions)
//! - `material`: Material definitions
//! - `profile`: Profile definitions (trace, via, layer, clearance constraints)
//! - `component`: Component definitions (metadata, pins, layout, electrical, render)
//! - `mechanical`: Mechanical definitions (dimensions, mounting holes, keepouts)
//! - `interface`: Interface definitions (bindings, protocols)
//! - `test`: Test definitions (setup, execute, assertions)
//! - `space`: Space definitions (dimensions, grid, origin, components, routes)
//! - `unit`: Unit definitions and measurement parsing

mod bridge;
mod component; // Now a modular subfolder with main, metadata, pins, layout, electrical, render, internal_pour
mod const_def;
mod device;
mod interface;
mod material;
mod mechanical;
mod module;
mod pattern;
mod profile;
pub mod shape; // Modular subfolder: parameters, points, generator, geometry, csg, helpers
mod signal_group;
mod space;
mod spice_model;
mod subcircuit;
mod test;
mod unit;

use super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::Token;

impl super::Parser {
    // ========================================================================
    // Top-Level Definition Parsing (v0.1.4)
    // ========================================================================

    /// Parse any definition: material, profile, component, module, mechanical, interface, test, unit, signal_group, pattern, strategy, or space
    ///
    /// v0.2.0: Supports optional `export` prefix for visibility control
    ///
    /// Reports errors to collector and returns None if parsing fails.
    pub(super) fn parse_definition(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<Definition> {
        // eprintln!(
        //     "[DEBUG] parse_definition called, current token: {:?}",
        //     self.current().map(|t| &t.token)
        // );

        // v0.2.0: Access control via 'export' keyword
        // Only definitions marked with 'export' are accessible from importing modules.
        // Definitions without 'export' are private to the module (file-local).
        let is_exported = if self.check(&Token::Export) {
            self.advance();
            true
        } else {
            false
        };

        // v0.1.6: No 'define' keyword - definitions start directly with type keyword

        // Check if next token is an identifier that matches our definition types
        if let Some(current) = self.current() {
            if let Token::Identifier(ident) = &current.token {
                match ident.as_str() {
                    "pattern" => {
                        // // eprintln!("[DEBUG] Parsing pattern definition");
                        self.advance();
                        return match self.parse_pattern() {
                            Ok(mut p) => {
                                p.is_exported = is_exported;
                                Some(Definition::Pattern(p))
                            }
                            Err(e) => {
                                collector.report(e);
                                None
                            }
                        };
                    }
                    "material_alias" => {
                        self.advance();
                        return match self.parse_material_alias() {
                            Ok(mut ma) => {
                                ma.is_exported = is_exported;
                                Some(Definition::MaterialAlias(ma))
                            }
                            Err(e) => {
                                collector.report(e);
                                None
                            }
                        };
                    }
                    "strategy" => {
                        // // eprintln!("[DEBUG] Parsing strategy definition");
                        self.advance();
                        return match self.parse_strategy() {
                            Ok(mut s) => {
                                s.is_exported = is_exported;
                                Some(Definition::Strategy(s))
                            }
                            Err(e) => {
                                collector.report(e);
                                None
                            }
                        };
                    }
                    _ => {}
                }
            }
        }

        let result = match self.current().map(|t| &t.token) {
            Some(Token::Bridge) => {
                // // eprintln!("[DEBUG] Dispatching to parse_bridge");
                self.parse_bridge(collector, is_exported)
                    .map(Definition::Bridge)
            }
            Some(Token::Material) => {
                // // eprintln!("[DEBUG] Dispatching to parse_material");
                self.parse_material(collector, is_exported)
                    .map(Definition::Material)
            }
            Some(Token::Profile) => {
                // // eprintln!("[DEBUG] Dispatching to parse_profile");
                self.parse_profile(collector, is_exported)
                    .map(|p| Definition::Profile(Box::new(p)))
            }
            Some(Token::Component) => {
                // // eprintln!("[DEBUG] Dispatching to parse_component_def");
                self.parse_component_def(collector, is_exported)
                    .map(Definition::Component)
            }
            Some(Token::Module) => {
                // // eprintln!("[DEBUG] Dispatching to parse_module");
                self.parse_module(collector, is_exported)
                    .map(Definition::Module)
            }
            Some(Token::Mechanical) => {
                // // eprintln!("[DEBUG] Dispatching to parse_mechanical");
                self.parse_mechanical(collector, is_exported)
                    .map(Definition::Mechanical)
            }
            Some(Token::Interface) => {
                // // eprintln!("[DEBUG] Dispatching to parse_interface");
                self.parse_interface(collector, is_exported)
                    .map(Definition::Interface)
            }
            Some(Token::Test) => {
                // // eprintln!("[DEBUG] Dispatching to parse_test");
                self.parse_test(collector, is_exported)
                    .map(Definition::Test)
            }
            Some(Token::Space) => {
                // // eprintln!("[DEBUG] Dispatching to parse_space");
                self.parse_space(collector, is_exported)
                    .map(Definition::Space)
            }
            Some(Token::Unit) => {
                // // eprintln!("[DEBUG] Dispatching to parse_unit");
                self.parse_unit(collector, is_exported)
                    .map(Definition::Unit)
            }
            Some(Token::Device) => {
                // // eprintln!("[DEBUG] Dispatching to parse_device");
                self.parse_device(collector, is_exported)
                    .map(Definition::Device)
            }
            Some(Token::Const) => {
                // // eprintln!("[DEBUG] Dispatching to parse_const");
                self.parse_const(collector, is_exported)
                    .map(Definition::Const)
            }
            Some(Token::SignalGroup) => {
                // // eprintln!("[DEBUG] Dispatching to parse_signal_group_definition");
                self.parse_signal_group_definition(collector, is_exported)
                    .map(Definition::SignalGroup)
            }
            Some(Token::Shape) => {
                // // eprintln!("[DEBUG] Dispatching to parse_shape");
                self.parse_shape(collector, is_exported)
                    .map(Definition::Shape)
            }
            Some(Token::SpiceModel) => {
                // // eprintln!("[DEBUG] Dispatching to parse_spice_model");
                self.parse_spice_model(collector, is_exported)
                    .map(Definition::SpiceModel)
            }
            Some(Token::Subcircuit) => {
                // v0.3.0: Native typed subcircuit definitions (replaces raw SPICE strings)
                self.parse_subcircuit(collector, is_exported)
                    .map(Definition::Subcircuit)
            }
            Some(Token::Logic) => {
                // // eprintln!("[DEBUG] Dispatching to parse_logic_definition");
                self.advance(); // consume 'logic' token
                match self.parse_logic_definition(is_exported) {
                    Ok(l) => Some(Definition::Logic(l)),
                    Err(e) => {
                        collector.report(e);
                        None
                    }
                }
            }
            Some(Token::Enum) => {
                // eprintln!("[DEBUG] Dispatching to parse_enum");
                match self.parse_enum(is_exported) {
                    Ok(e) => Some(Definition::Enum(e)),
                    Err(e) => {
                        collector.report(e);
                        None
                    }
                }
            }
            Some(Token::Struct) => {
                // eprintln!("[DEBUG] Dispatching to parse_struct");
                match self.parse_struct(is_exported) {
                    Ok(s) => Some(Definition::Struct(s)),
                    Err(e) => {
                        collector.report(e);
                        None
                    }
                }
            }
            _ => {
                // // eprintln!("[DEBUG] Unexpected token in parse_definition");
                collector.report(self.error(
                    "Expected definition type (bridge, material, profile, component, module, mechanical, interface, test, unit, device, const, signal_group, shape, spice_model, spice_subcircuit, logic, pattern, strategy, or space)",
                ));
                None
            }
        };

        // eprintln!(
        //     "[DEBUG] parse_definition returning: {}",
        //     if result.is_some() { "Some" } else { "None" }
        // );
        result
    }

    // ========================================================================
    // Import Parsing
    // ========================================================================

    /// Parse import statement: `import Name from @std/logic/gates` or `import Name from @org/package`
    /// Parse import statement (GAP3 - Three Import Modes):
    /// 1. Selective: `import A, B, C from @path`
    /// 2. Namespace: `import @path as Alias` (future)
    /// 3. Wildcard: `import * from @path`
    pub(super) fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::Import)?;

        // Parse import targets (what to import)
        let targets = self.parse_import_targets()?;

        // Expect "from" keyword
        self.expect(&Token::From)?;

        // Parse import source (where to import from)
        let path = self.parse_module_path()?;

        // Check for "as" alias (namespace import)
        let alias = if self.check(&Token::As) {
            self.advance(); // consume "as"
            Some(self.expect_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();
        let end_pos = self.previous_span().end;

        Ok(Import {
            targets,
            path,
            alias,
            span: crate::lexer::Span::new(start_pos, end_pos),
        })
    }

    /// Parse import targets: `*` or `A, B, C`
    fn parse_import_targets(&mut self) -> Result<ImportTargets, ParseError> {
        if self.check(&Token::Asterisk) {
            // Wildcard import: import * from @path
            self.advance();
            Ok(ImportTargets::Star)
        } else {
            // Selective import: import A, B, C from @path
            let mut names = vec![self.expect_identifier()?];

            // Parse comma-separated list
            while self.check(&Token::Comma) {
                self.advance(); // consume comma
                names.push(self.expect_identifier()?);
            }

            Ok(ImportTargets::List(names))
        }
    }

    /// Parse module path: `@std/logic/gates`, `logic/adders`, or `"Custom Path/Board.hw"`
    fn parse_module_path(&mut self) -> Result<ModulePath, ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::ImportPath(path_str) => {
                    // @org/package syntax
                    let path_str = path_str.clone();
                    self.advance();

                    // Remove the @ prefix
                    let path_without_at = path_str.trim_start_matches('@');

                    // Split by first slash to get org and rest of path
                    if let Some(slash_pos) = path_without_at.find('/') {
                        let org = path_without_at[..slash_pos].to_string();
                        let name = path_without_at[slash_pos + 1..].to_string();
                        Ok(ModulePath::Package {
                            org: org.into(),
                            name,
                        })
                    } else {
                        // Just @org with no path
                        Ok(ModulePath::Package {
                            org: path_without_at.to_string().into(),
                            name: String::new(),
                        })
                    }
                }
                Token::String(s) => {
                    // Quoted path: "Custom Path/Board.hw"
                    let path = s.clone();
                    self.advance();

                    // Check if quotes are unnecessary (no spaces in path)
                    if !path.contains(' ') {
                        // TODO: Add warning for unnecessary quotes
                        // For now, just accept it
                    }

                    Ok(ModulePath::Quoted(path))
                }
                Token::Range => {
                    // Parent directory path: ../shapes/hexagonal
                    let mut path_parts = Vec::new();
                    path_parts.push("..".to_string());
                    self.advance(); // consume '..'

                    // Continue collecting path components after slashes
                    while self.check(&Token::Slash) {
                        self.advance(); // consume '/'
                        path_parts.push(self.expect_identifier_or_keyword_string()?);
                    }

                    Ok(ModulePath::Relative(path_parts.join("/")))
                }
                Token::Identifier(_)
                | Token::Logic
                | Token::Test
                | Token::Component
                | Token::Space
                | Token::Material
                | Token::Profile
                | Token::Module
                | Token::Enum
                | Token::Struct
                | Token::Unit
                | Token::Device
                | Token::SignalGroup
                | Token::Shape
                | Token::Mechanical
                | Token::Interface => {
                    // Could be:
                    // 1. Bare identifier path: logic/adders (v0.1.6)
                    // (legacy dot-based standard.* removed in pre-release cleanup)

                    // Try to parse as path with slashes first
                    let start_ident = self.expect_identifier_or_keyword_string()?;

                    if self.check(&Token::Slash) {
                        // Bare identifier path: logic/adders
                        let mut path_parts = vec![start_ident];

                        while self.check(&Token::Slash) {
                            self.advance(); // consume slash
                            path_parts.push(self.expect_identifier_or_keyword_string()?);
                        }

                        Ok(ModulePath::Relative(path_parts.join("/")))
                    } else {
                        // Single identifier (no slashes or dots)
                        // (removed legacy dot syntax for standard.materials here; see ast/import.rs removal comment)
                        Ok(ModulePath::Relative(start_ident))
                    }
                }
                _ => Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected:
                        "import path (@org/package, logic/adders, ../parent, or \"quoted path\")"
                            .to_string()
                            .into(),
                    found: format!("{}", current.token).into(),
                }),
            }
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                miette::SourceSpan::new(0.into(), 0.into())
            };
            Err(ParseError::UnexpectedEof { span })
        }
    }
}
