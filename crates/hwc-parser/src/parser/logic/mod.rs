mod blocks;
mod definitions;
mod expressions;
mod statements;

use crate::parser::ParseError;
use crate::parser::Parser;

impl Parser {
    pub(super) fn expect_identifier_value(&mut self, expected: &str) -> Result<(), ParseError> {
        if let Some(token) = self.current() {
            if let crate::lexer::Token::Identifier(name) = &token.token {
                if name == expected {
                    self.advance();
                    return Ok(());
                }
            }
        }
        Err(self.error(&format!("Expected identifier '{}'", expected)))
    }
}
