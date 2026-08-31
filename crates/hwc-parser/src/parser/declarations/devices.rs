use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a device declaration: `device Name { type: DeviceType, ... }`
    pub fn parse_device_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<DeviceDecl, ParseError> {
        self.expect_token(&Token::Device, "Expected 'device'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for device body")?;
        let mut sections = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let sec_start = self.current_span().start;
            let sec_ident = self.expect_identifier()?;
            let sec_name: CompactString = sec_ident.name.as_str().into();

            if self.check(&Token::Colon) {
                self.advance();
                let expr = self.parse_expression()?;
                let end = expr.span().end;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                sections.push(DeviceSection {
                    name: sec_name,
                    fields: vec![("value".into(), expr)],
                    span: Span::new(sec_start, end),
                });
            } else if self.check(&Token::OpenBrace) {
                self.advance();
                let mut fields = Vec::new();
                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    let fld_ident = self.expect_identifier()?;
                    let fld_name: CompactString = fld_ident.name.as_str().into();
                    self.expect_token(&Token::Colon, "Expected ':' after device field")?;
                    let fld_val = self.parse_expression()?;
                    fields.push((fld_name, fld_val));
                    if self.check(&Token::Semicolon) || self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                let close_sec = self.expect_token(&Token::CloseBrace, "Expected '}' to close device section")?;
                sections.push(DeviceSection {
                    name: sec_name,
                    fields,
                    span: Span::new(sec_start, close_sec.end),
                });
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close device body")?;

        Ok(DeviceDecl {
            is_exported,
            name,
            sections,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
