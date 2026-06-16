use crate::ast::expr::{BinOp, Expr, UnaryOp};
use crate::ast::*;
use crate::lexer::Token;

impl crate::parser::Parser {
    pub(in crate::parser::definitions::shape) fn parse_geometry_blocks(
        &mut self,
    ) -> Result<Vec<GeometryBlock>, crate::parser::error::ParseError> {
        let mut blocks = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check_identifier("for") {
                blocks.push(self.parse_geometry_for_loop()?);
                self.skip_whitespace();
                continue;
            }

            if self.check_identifier("let") {
                blocks.push(self.parse_geometry_let()?);
                self.skip_whitespace();
                continue;
            }

            return Err(self.error("Expected 'for', 'let', or 'Point' in geometry block"));
        }

        if blocks.is_empty() {
            return Err(self.error("Geometry block must contain at least one statement"));
        }

        Ok(blocks)
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_for_loop(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        self.expect_identifier_named("for")?;

        let variable = self.expect_identifier()?.name.to_string();
        self.expect_identifier_named("in")?;

        let start = if let Some(current) = self.current() {
            if let Token::Integer(n) = &current.token {
                let val = *n;
                self.advance();
                val
            } else {
                return Err(self.error("Expected integer for loop start"));
            }
        } else {
            return Err(self.error("Expected integer for loop start"));
        };

        self.expect(&Token::Range)?;

        let end = if let Some(current) = self.current() {
            if let Token::Integer(n) = &current.token {
                let val = *n;
                self.advance();
                val
            } else {
                return Err(self.error("Expected integer for loop end"));
            }
        } else {
            return Err(self.error("Expected integer for loop end"));
        };

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;

        self.skip_whitespace();
        self.expect(&Token::Indent)?;

        let mut body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check_identifier("let") {
                let (name, value) = self.parse_geometry_let_statement()?;
                body.push(GeometryStatement::Variable { name, value });
            } else if self.check_identifier("Point") {
                body.push(self.parse_geometry_point_statement()?);
            } else {
                return Err(self.error("Expected 'let' or 'Point' in for loop body"));
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        if body.is_empty() {
            return Err(self.error("For loop body must contain at least one statement"));
        }

        Ok(GeometryBlock::ForLoop {
            variable,
            start,
            end,
            body,
        })
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_let(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        let (name, value) = self.parse_geometry_let_statement()?;
        Ok(GeometryBlock::Variable { name, value })
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_let_statement(
        &mut self,
    ) -> Result<(String, Expr), crate::parser::error::ParseError> {
        self.expect_identifier_named("let")?;

        let name = self.expect_identifier()?.name.to_string();
        self.expect(&Token::Equals)?;

        let value = self.parse_geometry_expr()?;
        Ok((name, value))
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_point_statement(
        &mut self,
    ) -> Result<GeometryStatement, crate::parser::error::ParseError> {
        self.expect_identifier_named("Point")?;
        self.expect(&Token::OpenParen)?;

        self.expect_identifier_named("x")?;
        self.expect(&Token::Colon)?;
        let x = self.parse_geometry_expr()?;

        self.expect(&Token::Comma)?;

        self.expect_identifier_named("y")?;
        self.expect(&Token::Colon)?;
        let y = self.parse_geometry_expr()?;

        self.expect(&Token::CloseParen)?;

        Ok(GeometryStatement::Point { x, y })
    }

    pub(super) fn parse_geometry_expr(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        self.parse_geometry_expr_if()
    }

    fn parse_geometry_expr_if(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check_identifier("if") {
            self.advance();
            let cond = self.parse_geometry_expr_comparison()?;
            self.expect(&Token::Colon)?;
            let then_branch = self.parse_geometry_expr()?;
            self.expect_identifier_named("else")?;
            self.expect(&Token::Colon)?;
            let else_branch = self.parse_geometry_expr()?;
            Ok(Expr::If {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            })
        } else {
            self.parse_geometry_expr_comparison()
        }
    }

    fn parse_geometry_expr_comparison(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_addition()?;

        let op = if self.check(&Token::Equals) {
            Some(BinOp::Eq)
        } else if self.check(&Token::NotEquals) {
            Some(BinOp::Ne)
        } else if self.check(&Token::LessThan) {
            Some(BinOp::Lt)
        } else if self.check(&Token::GreaterThan) {
            Some(BinOp::Gt)
        } else {
            None
        };

        if let Some(op) = op {
            self.advance();
            let right = self.parse_geometry_expr_addition()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_geometry_expr_addition(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_multiplication()?;

        loop {
            let op = if self.check(&Token::Plus) {
                Some(BinOp::Add)
            } else if self.check(&Token::Hyphen) {
                Some(BinOp::Sub)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_geometry_expr_multiplication()?;
                left = Expr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_geometry_expr_multiplication(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_unary()?;

        loop {
            let op = if self.check(&Token::Asterisk) {
                Some(BinOp::Mul)
            } else if self.check(&Token::Slash) {
                Some(BinOp::Div)
            } else if self.check(&Token::Mod) {
                Some(BinOp::Mod)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_geometry_expr_unary()?;
                left = Expr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_geometry_expr_unary(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check(&Token::Plus) {
            self.advance();
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Pos,
                expr: Box::new(expr),
            })
        } else if self.check(&Token::Hyphen) {
            self.advance();
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            })
        } else {
            self.parse_geometry_expr_call()
        }
    }

    fn parse_geometry_expr_call(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        let atom = self.parse_geometry_expr_atom()?;

        if let Expr::Identifier(ref name) = atom {
            if self.check(&Token::OpenParen) {
                let name = name.clone();
                self.advance();
                let mut args = Vec::new();

                if !self.check(&Token::CloseParen) && !self.is_at_end() {
                    args.push(self.parse_geometry_expr()?);
                    while self.check(&Token::Comma) {
                        self.advance();
                        args.push(self.parse_geometry_expr()?);
                    }
                }

                self.expect(&Token::CloseParen)?;
                return Ok(Expr::Call { name, args });
            }
        }

        Ok(atom)
    }

    fn parse_geometry_expr_atom(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Integer(n) => {
                    let val = *n as f64;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Float(n) => {
                    let val = *n;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Measurement(m) => {
                    let val = m.value;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Identifier(name) => {
                    let name = name.clone();
                    self.advance();
                    Ok(Expr::Identifier(name))
                }
                Token::OpenParen => {
                    self.advance();
                    let expr = self.parse_geometry_expr()?;
                    self.expect(&Token::CloseParen)?;
                    Ok(expr)
                }
                Token::True => {
                    self.advance();
                    Ok(Expr::Literal(1.0))
                }
                Token::False => {
                    self.advance();
                    Ok(Expr::Literal(0.0))
                }
                _ => Err(self.error("Expected expression")),
            }
        } else {
            Err(self.error("Expected expression"))
        }
    }
}
