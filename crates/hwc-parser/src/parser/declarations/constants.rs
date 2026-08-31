use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse a constant declaration: `(export)? const NAME: Type = value`
    pub fn parse_const_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<ConstDecl, ParseError> {
        self.expect_token(&Token::Const, "Expected 'const'")?;
        let name = self.expect_identifier()?;

        // Optional type annotation
        let type_annotation = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect_token(&Token::Equals, "Expected '=' after const name")?;
        let value = self.parse_expression()?;

        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(ConstDecl {
            is_exported,
            name,
            type_annotation,
            value,
            span: Span::new(start_pos, end_pos),
        })
    }
}
