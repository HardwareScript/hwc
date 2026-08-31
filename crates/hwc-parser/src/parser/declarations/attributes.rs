use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse an attribute: `#[name(arg1, arg2, ...)]` or `#[name]`
    pub fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let start_pos = self.current_span().start;
        self.expect_token(&Token::HashBracket, "Expected '#[' to begin attribute")?;
        let name = self.expect_identifier()?;

        let mut arguments = Vec::new();
        if self.check(&Token::OpenParen) {
            self.advance();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                arguments.push(self.parse_expression()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseParen, "Expected ')' after attribute arguments")?;
        }

        let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close attribute")?;
        Ok(Attribute {
            name,
            arguments,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse consecutive attributes: `(#[attr])*`
    pub fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }
}
