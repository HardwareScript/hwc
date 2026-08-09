//! Parser module for Hardware Script
//!
//! This module is organized into logical submodules:
//! - `error`: Parser error types with miette diagnostics
//! - `helpers`: Token navigation and utility methods
//! - `definitions`: Space, material, profile, and other definition parsing
//! - `components`: Component placement and coordinate parsing
//! - `routing`: Route and expose statement parsing

mod components;
mod context_errors; // New: Context-aware error generation
mod definitions; // Now a folder with submodules
mod error;
mod expression;
mod helpers;
mod logic;
mod routing;

pub use context_errors::{
    ContextErrorGenerator, ParsingContext, PlacementParseState, PourParseState, RouteParseState,
    SpaceParseState,
};
pub use error::ParseError;

use crate::ast::arena::AstArena;
use crate::ast::*;
use crate::lexer::SpannedToken;

/// Parser for Hardware Script with arena allocation
///
/// AST nodes are allocated into a type-safe AstArena referenced by u32 indices,
/// eliminating lifetime parameters entirely.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    current: usize,
    /// Context-aware error generator (Phase 1 refactor)
    error_context: ContextErrorGenerator,
    /// Type-safe arena allocator for AST nodes
    /// All large AST structures are allocated here for cache efficiency
    pub arena: AstArena,
}

impl Parser {
    /// Create a new parser from a token stream with arena allocation
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            current: 0,
            error_context: ContextErrorGenerator::new(),
            arena: AstArena::new(),
        }
    }

    /// Parse the token stream into a Program AST with multi-error reporting (v0.1.6).
    ///
    /// Instead of stopping at the first error, this method reports all errors
    /// to the collector and continues parsing. This enables TypeScript-like
    /// multi-error reporting for large designs.
    ///
    /// # Error Recovery Strategy
    ///
    /// When a definition fails to parse, the parser synchronizes to the next
    /// definition keyword and continues. This prevents cascading errors and
    /// allows the compiler to report all issues at once.
    pub fn parse(&mut self, collector: &crate::DiagnosticCollector) -> Program {
        let start_pos = if let Some(first) = self.tokens.first() {
            first.span.start
        } else {
            0
        };

        let mut imports = Vec::new();
        let mut re_exports = Vec::new();
        let mut definitions = Vec::new();

        // Skip any leading whitespace and comments
        self.skip_whitespace();

        // Parse top-level statements with error recovery
        while !self.is_at_end() {
            // eprintln!("[DEBUG] Main parser loop, current token: {:?}", self.current().map(|s| &s.token));

            // Collect doc comments (they'll be attached to next statement)
            let _doc_comments = self.collect_doc_comments();

            // Skip remaining whitespace
            self.skip_whitespace();

            // Check if we should stop (hit error limit)
            if collector.should_stop() {
                // eprintln!("[DEBUG] Hit error limit, stopping parse");
                break;
            }

            // v0.1.6: Check for import, type keywords (component, space, etc.), enum, or struct
            // v0.2.0: Check for 'export' at top level (can be re-export or definition)
            if self.check(&crate::lexer::Token::Import) {
                // eprintln!("[DEBUG] Parsing import statement");
                match self.parse_import() {
                    Ok(import) => imports.push(import),
                    Err(e) => {
                        collector.report(e);
                        self.sync_to_next_definition();
                    }
                }
            } else if self.check(&crate::lexer::Token::Export) {
                // v0.2.0: 'export' can be either:
                // 1. Re-export: `export SymbolName` (standalone line)
                // 2. Exported definition: `export material Name:` (definition follows)

                let export_start = self.current;
                self.advance(); // consume 'export'

                // Check what comes after 'export'
                let is_reexport = if let Some(current) = self.current() {
                    // Check if this is just an identifier (re-export) vs definition keyword
                    matches!(current.token, crate::lexer::Token::Identifier(_)) && {
                        // Peek ahead - if there's no colon next, it's a re-export
                        let next_idx = self.current + 1;
                        next_idx >= self.tokens.len()
                            || !matches!(
                                self.tokens.get(next_idx).map(|t| &t.token),
                                Some(crate::lexer::Token::Colon)
                            )
                    }
                } else {
                    false
                };

                if is_reexport {
                    // This is a re-export: `export SymbolName`
                    if let Ok(ident) = self.expect_identifier() {
                        let end_pos = self.previous_span().end;
                        re_exports.push(ReExport {
                            symbol: ident,
                            span: crate::lexer::Span::new(export_start, end_pos),
                        });
                    } else {
                        collector
                            .report(self.error("Expected identifier after 'export' for re-export"));
                        self.sync_to_next_definition();
                    }
                } else {
                    // This is an exported definition: `export material Name:`, etc.
                    // Reset position to before 'export' and let parse_definition handle it
                    self.current = export_start;

                    if let Some(def) = self.parse_definition(collector) {
                        definitions.push(def);
                    } else {
                        self.sync_to_next_definition();
                    }
                }
            } else if self.check(&crate::lexer::Token::Component)
                || self.check(&crate::lexer::Token::Space)
                || self.check(&crate::lexer::Token::Material)
                || self.check(&crate::lexer::Token::Profile)
                || self.check(&crate::lexer::Token::Module)
                || self.check(&crate::lexer::Token::Mechanical)
                || self.check(&crate::lexer::Token::Interface)
                || self.check(&crate::lexer::Token::Test)
                || self.check(&crate::lexer::Token::Unit)
                || self.check(&crate::lexer::Token::Device)
                || self.check(&crate::lexer::Token::Const)
                || self.check(&crate::lexer::Token::SignalGroup)
                || self.check(&crate::lexer::Token::Shape)
                || self.check(&crate::lexer::Token::SpiceModel)
                || self.check(&crate::lexer::Token::Subcircuit)
                || self.check(&crate::lexer::Token::Logic)
                || self.check(&crate::lexer::Token::Enum)
                || self.check(&crate::lexer::Token::Struct)
                || self.check(&crate::lexer::Token::Export)
            {
                // eprintln!("[DEBUG] Parsing definition: {:?}", self.current().map(|s| &s.token));
                // Parse definition with error recovery (handles export keyword internally)
                if let Some(def) = self.parse_definition(collector) {
                    // eprintln!("[DEBUG] Definition parsed successfully");
                    definitions.push(def);
                } else {
                    // eprintln!("[DEBUG] Definition parse failed, syncing to next");
                    self.sync_to_next_definition();
                }
            } else if let Some(current) = self.current() {
                // eprintln!("[DEBUG] Checking for pattern/strategy or unrecognized token");
                // v0.1.6: Check for pattern and strategy as identifiers
                if let crate::lexer::Token::Identifier(name) = &current.token {
                    if name == "pattern" || name == "strategy" || name == "material_alias" {
                        // eprintln!("[DEBUG] Parsing pattern/strategy/material_alias");
                        if let Some(def) = self.parse_definition(collector) {
                            definitions.push(def);
                        } else {
                            self.sync_to_next_definition();
                        }
                        continue;
                    }

                    // v0.1.6 Migration: Detect 'define' keyword (removed in v0.1.6)
                    if name == "define" {
                        // eprintln!("[DEBUG] Found deprecated 'define' keyword");
                        collector.report(crate::parser::error::error_define_keyword_removed(
                            &current.span,
                        ));
                        self.sync_to_next_definition();
                        continue;
                    }
                }

                // Not a recognized top-level construct
                if self.check(&crate::lexer::Token::Newline) {
                    // eprintln!("[DEBUG] Skipping newline");
                    self.advance();
                } else if self.check(&crate::lexer::Token::Eof) {
                    // eprintln!("[DEBUG] Reached EOF");
                    break;
                } else {
                    // Professional, authoritative error message
                    let found = format!("{}", current.token);
                    // eprintln!("[DEBUG] Unrecognized token at top level: {}", found);
                    collector.report(self.error(&format!("'{}' not recognized at top level. Hardware Script files must start with 'import' or a definition type (component.into(), space, material, profile, module, enum, struct, etc.).", found)));
                    self.sync_to_next_definition();
                }
            } else {
                // eprintln!("[DEBUG] No current token, breaking");
                break;
            }
        }

        let end_pos = if let Some(last) = self.tokens.last() {
            last.span.end
        } else {
            0
        };

        let arena = std::mem::take(&mut self.arena);

        Program {
            imports,
            re_exports,
            definitions,
            arena,
            span: crate::lexer::Span::new(start_pos, end_pos),
        }
    }

    /// Synchronize to the next definition after a parse error.
    ///
    /// This method skips tokens until it finds a definition keyword,
    /// enabling error recovery and multi-error reporting.
    fn sync_to_next_definition(&mut self) {
        // eprintln!("[DEBUG] sync_to_next_definition called at position {}", self.current);

        // CRITICAL: Always advance at least once to guarantee progress
        if !self.is_at_end() {
            // eprintln!("[DEBUG] Advancing from token: {:?}", self.current().map(|s| &s.token));
            self.advance();
        }

        // eprintln!("[DEBUG] Searching for next definition keyword...");
        let mut iterations = 0;
        while let Some(token) = self.current() {
            iterations += 1;
            if iterations % 10 == 0 {
                // eprintln!("[DEBUG] sync iteration {}: current token = {:?}", iterations, token.token);
            }
            if iterations > 100 {
                // eprintln!("[DEBUG] WARNING: sync_to_next_definition has iterated {} times!", iterations);
            }
            if iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: sync_to_next_definition infinite loop detected! Breaking.");
                break;
            }

            match &token.token {
                crate::lexer::Token::Space => {
                    // Only stop if followed by an identifier (space SpaceName), not a dot (space.anchor)
                    if let Some(next) = self.peek_ahead(1) {
                        if matches!(next.token, crate::lexer::Token::Identifier(_)) {
                            break;
                        } else {
                            self.advance();
                        }
                    } else {
                        break;
                    }
                }
                crate::lexer::Token::Component
                | crate::lexer::Token::Material
                | crate::lexer::Token::Profile
                | crate::lexer::Token::Module
                | crate::lexer::Token::Mechanical
                | crate::lexer::Token::Interface
                | crate::lexer::Token::Test
                | crate::lexer::Token::Unit
                | crate::lexer::Token::Device
                | crate::lexer::Token::Const
                | crate::lexer::Token::SignalGroup
                | crate::lexer::Token::Shape
                | crate::lexer::Token::SpiceModel
                | crate::lexer::Token::Subcircuit
                | crate::lexer::Token::Logic
                | crate::lexer::Token::Enum
                | crate::lexer::Token::Struct
                | crate::lexer::Token::Import => {
                    // eprintln!("[DEBUG] Found definition keyword: {:?}, stopping sync", token.token);
                    break;
                }
                crate::lexer::Token::Identifier(name)
                    if name == "pattern" || name == "strategy" || name == "material_alias" =>
                {
                    // eprintln!("[DEBUG] Found pattern/strategy/material_alias, stopping sync");
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }
        // eprintln!("[DEBUG] sync_to_next_definition complete, now at: {:?}", self.current().map(|s| &s.token));
    }
}
