use crate::ast::*;
use crate::lexer::Token;

impl<'ast> crate::parser::Parser<'ast> {
    pub(in crate::parser::definitions::shape) fn check_csg_operator(&self) -> bool {
        if let Some(current) = self.current() {
            if let Token::Identifier(_) = &current.token {
                let pos = self.current;
                if pos + 1 < self.tokens.len() {
                    if let Token::Plus | Token::Hyphen | Token::Asterisk =
                        &self.tokens[pos + 1].token
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(in crate::parser::definitions::shape) fn parse_csg_expression(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        self.parse_csg_term()
    }

    pub(in crate::parser::definitions::shape) fn parse_csg_term(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        let mut left = self.parse_csg_factor()?;

        loop {
            self.skip_whitespace();

            if self.check(&Token::Plus) {
                self.advance();
                let right = self.parse_csg_factor()?;
                left = CsgExpression::Union(Box::new(left), Box::new(right));
            } else if self.check(&Token::Hyphen) {
                if self.is_binary_minus() {
                    self.advance();
                    let right = self.parse_csg_factor()?;
                    left = CsgExpression::Difference(Box::new(left), Box::new(right));
                } else {
                    break;
                }
            } else if self.check(&Token::Asterisk) {
                self.advance();
                let right = self.parse_csg_factor()?;
                left = CsgExpression::Intersection(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    pub(in crate::parser::definitions::shape) fn parse_csg_factor(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        let mut expr = self.parse_csg_atom()?;

        loop {
            self.skip_whitespace();

            if self.check_identifier("rotated") {
                self.advance();
                let rotation = self.parse_rotation_value()?;
                expr = CsgExpression::Transformed {
                    expr: Box::new(expr),
                    rotation: Some(rotation),
                    translation: None,
                };
            } else if self.check(&Token::At) {
                self.advance();
                let translation = self.parse_translation()?;
                expr = CsgExpression::Transformed {
                    expr: Box::new(expr),
                    rotation: None,
                    translation: Some(translation),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    pub(in crate::parser::definitions::shape) fn parse_csg_atom(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        self.skip_whitespace();

        if self.check_identifier("Rectangle") {
            self.advance();
            self.expect(&Token::OpenParen)?;

            self.expect_identifier_named("width")?;
            self.expect(&Token::Colon)?;
            let width = self.read_expression_until_comma_or_close()?;

            self.expect(&Token::Comma)?;

            self.expect_identifier_named("height")?;
            self.expect(&Token::Colon)?;
            let height = self.read_expression_until_comma_or_close()?;

            self.expect(&Token::CloseParen)?;

            return Ok(CsgExpression::Primitive(CsgPrimitive::Rectangle {
                width,
                height,
            }));
        }

        if self.check_identifier("Circle") {
            self.advance();
            self.expect(&Token::OpenParen)?;

            self.expect_identifier_named("diameter")?;
            self.expect(&Token::Colon)?;
            let diameter = self.read_expression_until_comma_or_close()?;

            self.expect(&Token::CloseParen)?;

            return Ok(CsgExpression::Primitive(CsgPrimitive::Circle { diameter }));
        }

        if self.check(&Token::OpenParen) {
            self.advance();
            let expr = self.parse_csg_expression()?;
            self.expect(&Token::CloseParen)?;
            return Ok(expr);
        }

        if let Some(current) = self.current() {
            if let Token::Identifier(name) = &current.token {
                let name = name.clone();
                self.advance();
                return Ok(CsgExpression::Primitive(CsgPrimitive::ShapeRef(name)));
            }
        }

        Err(self
            .error("Expected CSG expression: Rectangle(...), Circle(...), identifier, or (expr)"))
    }

    fn parse_rotation_value(&mut self) -> Result<f64, crate::parser::error::ParseError> {
        self.skip_whitespace();

        if let Some(current) = self.current() {
            match &current.token {
                Token::Float(n) => {
                    let value = *n;
                    self.advance();
                    if self.check_identifier("deg") {
                        self.advance();
                    }
                    return Ok(value);
                }
                Token::Integer(n) => {
                    let value = *n as f64;
                    self.advance();
                    if self.check_identifier("deg") {
                        self.advance();
                    }
                    return Ok(value);
                }
                Token::Measurement(m) => {
                    let value = m.value;
                    self.advance();
                    return Ok(value);
                }
                _ => {}
            }
        }

        Err(self.error("Expected rotation value (e.g., 22.5deg)"))
    }

    fn parse_translation(&mut self) -> Result<(f64, f64), crate::parser::error::ParseError> {
        self.skip_whitespace();
        self.expect(&Token::OpenBracket)?;

        self.expect_identifier_named("x")?;
        self.expect(&Token::Colon)?;
        let x_str = self.read_expression_until_comma_or_close()?;
        let x = x_str
            .parse::<f64>()
            .map_err(|_| self.error("Expected number for x translation"))?;

        self.expect(&Token::Comma)?;

        self.expect_identifier_named("y")?;
        self.expect(&Token::Colon)?;
        let y_str = self.read_expression_until_comma_or_close()?;
        let y = y_str
            .parse::<f64>()
            .map_err(|_| self.error("Expected number for y translation"))?;

        self.expect(&Token::CloseBracket)?;

        Ok((x, y))
    }

    pub(in crate::parser::definitions::shape) fn lookahead_is_csg_let(&self) -> bool {
        if !self.check_identifier("let") {
            return false;
        }

        let mut pos = self.current;
        if pos < self.tokens.len() {
            pos += 1;
        }

        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Identifier(_) => {
                    pos += 1;
                    break;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }

        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Equals => {
                    pos += 1;
                    break;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }

        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Identifier(name) if name == "Rectangle" || name == "Circle" => {
                    return true;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }

        false
    }

    pub(in crate::parser::definitions::shape) fn parse_csg_with_let_bindings(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        let mut body = None;

        loop {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check_identifier("let") {
                self.advance();

                let var_name = self.expect_identifier()?.name.to_string();
                self.expect(&Token::Equals)?;

                let expr = self.parse_csg_factor()?;
                body = Some((var_name, expr));
            } else {
                break;
            }
        }

        let final_expr = self.parse_csg_term()?;

        if let Some((name, value)) = body {
            Ok(CsgExpression::LetBinding {
                name,
                value: Box::new(value),
                body: Box::new(final_expr),
            })
        } else {
            Ok(final_expr)
        }
    }
}
