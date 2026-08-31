use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse an export list: `export { Item1, Item2, Item3 }`
    pub fn parse_export_list(&mut self, start_pos: usize) -> Result<ExportDecl, ParseError> {
        self.expect_token(&Token::OpenBrace, "Expected '{' for export list")?;

        let mut symbols = Vec::new();
        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let ident = self.expect_identifier()?;
            symbols.push(CompactString::from(ident.name.as_str()));
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' after export symbol list")?;

        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(ExportDecl {
            symbols,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
