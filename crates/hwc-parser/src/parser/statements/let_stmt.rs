use crate::ast::{BindingPattern, Statement};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    pub(super) fn parse_let_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Let, "Expected 'let'")?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let pattern = if self.check(&Token::OpenParen) {
            self.advance();
            let mut vars = Vec::new();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                if self.check(&Token::Underscore) {
                    self.advance();
                    vars.push(CompactString::from("_"));
                } else {
                    let ident = self.expect_identifier()?;
                    vars.push(CompactString::from(ident.name.as_str()));
                }
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseParen, "Expected ')' to close tuple binding pattern")?;
            BindingPattern::Tuple(vars)
        } else if self.check(&Token::Underscore) {
            self.advance();
            BindingPattern::Identifier("_".into())
        } else {
            let ident = self.expect_identifier()?;
            BindingPattern::Identifier(ident.name.as_str().into())
        };

        let type_annotation = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect_token(&Token::Equals, "Expected '=' in let statement")?;
        let value = self.parse_expression()?;
        let end_pos = value.span().end;

        Ok(Statement::Let {
            mutable,
            pattern,
            type_annotation,
            value,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }
}
