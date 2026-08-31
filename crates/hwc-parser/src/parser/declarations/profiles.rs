use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a profile declaration: `profile Name { section Name { prop: val } }`
    pub fn parse_profile_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<ProfileDecl, ParseError> {
        self.expect_token(&Token::Profile, "Expected 'profile'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for profile body")?;
        let mut sections = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let sec_start = self.current_span().start;
            let sec_type_ident = self.expect_identifier()?;
            let sec_type: CompactString = sec_type_ident.name.as_str().into();

            // Section name can be an identifier or string literal
            let sec_name = if !self.check(&Token::OpenBrace) {
                if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
                    let name = s.clone();
                    self.advance();
                    Some(CompactString::from(name))
                } else {
                    let id = self.expect_identifier()?;
                    Some(CompactString::from(id.name.as_str()))
                }
            } else {
                None
            };

            self.expect_token(&Token::OpenBrace, "Expected '{' for profile section body")?;
            let mut fields = Vec::new();

            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                // Check if this is a nested subsection (identifier followed by {)
                // or a field (identifier followed by :)
                let is_subsection = if let Some(Token::Identifier(_)) = self.current().map(|t| &t.token) {
                    // Look ahead to see if next token after identifier is '{' or ':'
                    if self.current + 1 < self.tokens.len() {
                        matches!(self.tokens[self.current + 1].token, Token::OpenBrace)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_subsection {
                    // Parse nested subsection: layer_name { field: value, ... }
                    let subsec_start = self.current_span().start;
                    let subsec_ident = self.expect_identifier()?;
                    let subsec_name: CompactString = subsec_ident.name.as_str().into();

                    self.expect_token(&Token::OpenBrace, "Expected '{' for nested subsection")?;
                    let mut subsec_fields = Vec::new();

                    while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                        let fld_ident = self.expect_identifier()?;
                        let fld_name: CompactString = fld_ident.name.as_str().into();
                        self.expect_token(&Token::Colon, "Expected ':' after field name")?;
                        let fld_val = self.parse_expression()?;
                        subsec_fields.push((fld_name, fld_val));

                        if self.check(&Token::Semicolon) {
                            self.advance();
                        }
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }

                    let subsec_close = self.expect_token(&Token::CloseBrace, "Expected '}' to close nested subsection")?;

                    // Add the nested subsection as a special field entry
                    // Store it as a struct-like expression
                    let subsec_expr = Expression::StructInstance {
                        name: subsec_name.clone(),
                        fields: subsec_fields.into_iter().map(|(k, v)| {
                            let span = v.span();
                            FieldInit {
                                name: k,
                                value: Some(v),
                                span,
                            }
                        }).collect(),
                        span: Span::new(subsec_start, subsec_close.end),
                    };
                    fields.push((subsec_name, subsec_expr));
                } else {
                    // Parse regular field: field_name: value
                    let fld_ident = self.expect_identifier()?;
                    let fld_name: CompactString = fld_ident.name.as_str().into();
                    self.expect_token(&Token::Colon, "Expected ':' after profile field name")?;
                    let fld_val = self.parse_expression()?;
                    fields.push((fld_name, fld_val));

                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }
                }
            }

            let sec_close = self.expect_token(&Token::CloseBrace, "Expected '}' to close profile section")?;
            sections.push(ProfileSection {
                section_type: sec_type,
                name: sec_name,
                fields,
                span: Span::new(sec_start, sec_close.end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close profile body")?;

        Ok(ProfileDecl {
            is_exported,
            name,
            sections,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
