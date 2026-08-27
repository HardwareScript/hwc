//! Parser module for HardwareScript v0.3.0

pub mod declarations;
pub mod error;
pub mod expression;
pub mod helpers;
pub mod statements;

pub use error::ParseError;

use crate::ast::*;
use crate::lexer::{Span, SpannedToken, Token};

/// Parser for HardwareScript v0.3.0
pub struct Parser {
    pub(crate) tokens: Vec<SpannedToken>,
    pub(crate) current: usize,
}

impl Parser {
    /// Create a new parser from a token stream
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, current: 0 }
    }

    /// Parse token stream into a complete Program AST with diagnostic reporting and panic mode recovery
    pub fn parse(&mut self, collector: &crate::DiagnosticCollector) -> Program {
        let start_pos = if let Some(first) = self.tokens.first() {
            first.span.start
        } else {
            0
        };

        let mut imports = Vec::new();
        let mut items = Vec::new();

        while !self.is_at_end() {
            // Semicolons at top level can be safely skipped
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.is_at_end() {
                break;
            }

            if collector.should_stop() {
                break;
            }

            let start_item_pos = self.current_span().start;
            let is_exported = if self.check(&Token::Export) {
                self.advance();
                // Check if this is `export { ... }` re-export syntax
                if self.check(&Token::OpenBrace) {
                    match self.parse_export_list(start_item_pos) {
                        Ok(exp) => items.push(TopLevelItem::Export(exp)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                    continue;
                }
                true
            } else {
                false
            };

            match self.current().map(|t| &t.token) {
                Some(Token::Import) => {
                    match self.parse_import() {
                        Ok(imp) => imports.push(imp),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Fn) => {
                    match self.parse_function_decl(is_exported, start_item_pos) {
                        Ok(f) => items.push(TopLevelItem::Function(f)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Struct) => {
                    match self.parse_struct_decl(is_exported, start_item_pos) {
                        Ok(s) => items.push(TopLevelItem::Struct(s)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Enum) => {
                    match self.parse_enum_decl(is_exported, start_item_pos) {
                        Ok(en) => items.push(TopLevelItem::Enum(en)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Const) => {
                    match self.parse_const_decl(is_exported, start_item_pos) {
                        Ok(c) => items.push(TopLevelItem::Const(c)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Space) => {
                    match self.parse_space_decl(start_item_pos) {
                        Ok(sp) => items.push(TopLevelItem::Space(sp)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Module) => {
                    match self.parse_module_decl(start_item_pos) {
                        Ok(m) => items.push(TopLevelItem::Module(m)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Material) => {
                    match self.parse_material_decl(is_exported, start_item_pos) {
                        Ok(mat) => items.push(TopLevelItem::Material(mat)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Profile) => {
                    match self.parse_profile_decl(is_exported, start_item_pos) {
                        Ok(prof) => items.push(TopLevelItem::Profile(prof)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Device) => {
                    match self.parse_device_decl(is_exported, start_item_pos) {
                        Ok(dev) => items.push(TopLevelItem::Device(dev)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(Token::Test) => {
                    match self.parse_test_decl(start_item_pos) {
                        Ok(tst) => items.push(TopLevelItem::Test(tst)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                Some(_) => {
                    // Top-level statement (e.g., let, println(...), for, if, etc.)
                    match self.parse_statement() {
                        Ok(stmt) => items.push(TopLevelItem::Statement(stmt)),
                        Err(e) => {
                            collector.report(e);
                            self.synchronize_top_level();
                        }
                    }
                }
                None => break,
            }
        }

        let end_pos = if let Some(last) = self.tokens.last() {
            last.span.end
        } else {
            0
        };

        Program {
            imports,
            items,
            span: Span::new(start_pos, end_pos),
        }
    }

    /// Panic mode error recovery: skip tokens until next top-level item keyword
    pub fn synchronize_top_level(&mut self) {
        if !self.is_at_end() {
            self.advance();
        }

        while !self.is_at_end() {
            if let Some(token) = self.current() {
                match token.token {
                    Token::Import
                    | Token::Export
                    | Token::Fn
                    | Token::Struct
                    | Token::Enum
                    | Token::Space
                    | Token::Module
                    | Token::Material
                    | Token::Profile
                    | Token::Device
                    | Token::Test
                    | Token::Let
                    | Token::For
                    | Token::If
                    | Token::Match
                    | Token::Assert
                    | Token::Semicolon => break,
                    _ => self.advance(),
                }
            } else {
                break;
            }
        }
    }
}
