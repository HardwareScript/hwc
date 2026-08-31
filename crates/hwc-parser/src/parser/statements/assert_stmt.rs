use crate::ast::Statement;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    pub(super) fn parse_assert_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Assert, "Expected 'assert'")?;
        self.expect_token(&Token::OpenParen, "Expected '(' after assert")?;

        let condition = self.parse_expression()?;
        let mut message = None;
        let mut args = Vec::new();

        if self.check(&Token::Comma) {
            self.advance();
            if let Some(Token::String(msg)) = self.current().map(|t| &t.token) {
                message = Some(msg.clone());
                self.advance();

                while self.check(&Token::Comma) {
                    self.advance();
                    args.push(self.parse_expression()?);
                }
            } else {
                args.push(self.parse_expression()?);
            }
        }

        let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close assert")?;

        Ok(Statement::Assert {
            condition,
            message,
            args,
            span: crate::ast::Span::new(start_pos, close_span.end),
        })
    }
}
