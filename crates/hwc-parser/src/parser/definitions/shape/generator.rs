use crate::lexer::Token;
use rustc_hash::FxHashMap;

use crate::parser::error::ParseError;
use crate::ShapeGenerator;

impl crate::parser::Parser {
    pub(in crate::parser::definitions::shape) fn parse_shape_generator(
        &mut self,
    ) -> Result<ShapeGenerator, ParseError> {
        let gen_name = self.expect_identifier()?.name;

        self.expect(&Token::OpenParen)?;

        let mut params = FxHashMap::default();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseParen) {
                break;
            }

            let param_name = self.expect_identifier()?.name;
            self.expect(&Token::Colon)?;
            let param_value = self.read_expression_until_comma_or_close()?;

            params.insert(param_name.to_string(), param_value);

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
            } else if !self.check(&Token::CloseParen) {
                return Err(self.error("Expected ',' or ')' in generator parameters"));
            }
        }

        self.expect(&Token::CloseParen)?;

        Ok(ShapeGenerator {
            name: gen_name.to_string(),
            params,
        })
    }
}
