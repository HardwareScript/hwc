use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse an import declaration:
    /// `import * from @std/primitives/units`
    /// `import { sky130_nmos, pad } from @std/layout/sky130`
    /// `import "path/to/file"`
    pub fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        let start_pos = self.current_span().start;
        self.expect_token(&Token::Import, "Expected 'import'")?;

        let symbols = if self.check(&Token::Asterisk) {
            self.advance(); // consume `*`
            ImportSymbols::All
        } else if self.check(&Token::OpenBrace) {
            self.advance(); // consume `{`
            let mut list = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                let ident = self.expect_identifier()?;
                list.push(CompactString::from(ident.name.as_str()));
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseBrace, "Expected '}' after import symbol list")?;
            ImportSymbols::Named(list)
        } else if let Some(Token::ImportPath(path)) = self.current().map(|t| &t.token) {
            let path_str = path.clone();
            let end_pos = self.current_span().end;
            self.advance();
            return Ok(ImportDecl {
                symbols: ImportSymbols::All,
                from: path_str,
                span: Span::new(start_pos, end_pos),
            });
        } else if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
            let path_str = s.clone();
            let end_pos = self.current_span().end;
            self.advance();
            return Ok(ImportDecl {
                symbols: ImportSymbols::All,
                from: path_str,
                span: Span::new(start_pos, end_pos),
            });
        } else {
            let ident = self.expect_identifier()?;
            ImportSymbols::Single(ident.name.as_str().into())
        };

        self.expect_token(&Token::From, "Expected 'from' after import symbols")?;

        let (from_path, end_pos) = if let Some(Token::ImportPath(path)) = self.current().map(|t| &t.token) {
            let p = path.clone();
            let end = self.current_span().end;
            self.advance();
            (p, end)
        } else if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
            let p = s.clone();
            let end = self.current_span().end;
            self.advance();
            (p, end)
        } else {
            let ident = self.expect_identifier()?;
            let end = self.previous_span().end;
            (ident.name.to_string(), end)
        };

        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(ImportDecl {
            symbols,
            from: from_path,
            span: Span::new(start_pos, end_pos),
        })
    }
}
