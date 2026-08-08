use crate::lexer::Token;

use crate::ShapeParameter;

impl<'ast> crate::parser::Parser<'ast> {
    pub(in crate::parser::definitions::shape) fn parse_shape_parameters(
        &mut self,
    ) -> Result<Vec<ShapeParameter>, crate::parser::error::ParseError> {
        let mut parameters = Vec::new();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseParen) {
                break;
            }

            let param_name = self.expect_identifier()?;

            self.expect(&Token::Colon)?;

            self.expect_identifier()?;

            let default_value = if self.check(&Token::Equals) {
                self.advance();
                Some(self.read_expression_string()?)
            } else {
                None
            };

            parameters.push(ShapeParameter {
                name: param_name,
                default_value,
            });

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance();
            } else if !self.check(&Token::CloseParen) {
                return Err(self.error("Expected ',' or ')' in shape parameters"));
            }
        }

        Ok(parameters)
    }
}
