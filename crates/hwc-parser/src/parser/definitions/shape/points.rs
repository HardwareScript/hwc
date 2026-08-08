use crate::lexer::Token;

use crate::ShapePoint;

impl<'ast> crate::parser::Parser<'ast> {
    pub(in crate::parser::definitions::shape) fn parse_shape_points(
        &mut self,
    ) -> Result<Vec<ShapePoint>, crate::parser::error::ParseError> {
        let mut points = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            self.expect(&Token::Hyphen)?;
            self.expect(&Token::OpenBracket)?;

            self.expect_identifier_named("x")?;
            self.expect(&Token::Colon)?;
            let x_expr = self.read_expression_until(&Token::Comma)?;

            self.expect(&Token::Comma)?;

            self.expect_identifier_named("y")?;
            self.expect(&Token::Colon)?;
            let y_expr = self.read_expression_until(&Token::CloseBracket)?;

            self.expect(&Token::CloseBracket)?;

            points.push(ShapePoint { x_expr, y_expr });

            self.skip_whitespace();
        }

        if points.is_empty() {
            return Err(self.error("Shape must have at least one point"));
        }

        Ok(points)
    }
}
