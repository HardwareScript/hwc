use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::ParseError;
use crate::parser::Parser;

impl<'ast> Parser<'ast> {
    pub(super) fn parse_logic_statement(&mut self) -> Result<LogicStatement, ParseError> {
        if let Some(Token::Identifier(name)) = self.current().map(|t| &t.token) {
            if name == "pass" {
                self.advance();
                self.consume_statement_end()?;
                return Ok(LogicStatement::If {
                    condition: LogicExpression::Boolean {
                        value: false,
                        span: self.previous_span(),
                    },
                    then_block: BlockOrExpr::Pass(self.previous_span()),
                    else_block: None,
                    span: self.previous_span(),
                });
            }
        }

        if self.check(&Token::Let) {
            self.parse_let_statement()
        } else if self.check(&Token::If) {
            self.parse_if_statement()
        } else {
            self.parse_assignment_or_expression_statement()
        }
    }

    fn parse_assignment_or_expression_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let _start = self.current_span();

        let checkpoint = self.current;

        let is_assignment = if let Some(Token::Identifier(..)) = self.current().map(|t| &t.token) {
            self.advance();
            let has_accessor = self.check(&Token::Dot) || self.check(&Token::OpenBracket);
            if has_accessor {
                self.advance();
                if matches!(
                    self.current().map(|t| &t.token),
                    Some(Token::Identifier(..)) | Some(Token::Integer(..))
                ) {
                    self.advance();
                }
                if self.check(&Token::CloseBracket) {
                    self.advance();
                }
            }
            let result = self.check(&Token::Equals);
            self.current = checkpoint;
            result
        } else {
            false
        };

        if is_assignment {
            self.parse_assignment_statement()
        } else {
            let expression = self.parse_logic_expression()?;
            self.consume_statement_end()?;

            Ok(LogicStatement::Expression(expression))
        }
    }

    fn parse_let_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Let)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.expect_identifier_string()?;

        let width = if self.check(&Token::OpenBracket) {
            self.advance();
            let w = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Some(w)
        } else {
            None
        };

        self.expect(&Token::Equals)?;

        let expression = self.parse_logic_expression()?;

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::Let {
            mutable,
            name: name.into(),
            width,
            expression,
            span,
        })
    }

    fn parse_assignment_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        let target = self.parse_assignment_target()?;

        self.expect(&Token::Equals)?;

        let expression = self.parse_logic_expression()?;

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::Assignment {
            target,
            expression,
            span,
        })
    }

    fn parse_assignment_target(&mut self) -> Result<AssignmentTarget, ParseError> {
        let start = self.current_span();
        let name = self.expect_identifier_string()?;

        if self.check(&Token::Dot) {
            self.advance();
            let field = self.expect_identifier()?;

            if field.as_str() == "next" {
                let span = Span::new(start.start, self.previous_span().end);
                Ok(AssignmentTarget::RegisterNext {
                    name: name.into(),
                    span,
                })
            } else {
                Err(self.error(&format!(
                    "Invalid field '{}' in assignment target. Only '.next' is allowed for registers",
                    field
                )))
            }
        } else if self.check(&Token::OpenBracket) {
            self.advance();
            let range = self.parse_range()?;
            self.expect(&Token::CloseBracket)?;
            let span = Span::new(start.start, self.previous_span().end);
            Ok(AssignmentTarget::Slice {
                name: name.into(),
                range,
                span,
            })
        } else {
            let span = Span::new(start.start, self.previous_span().end);
            Ok(AssignmentTarget::Variable {
                name: name.into(),
                span,
            })
        }
    }

    fn parse_if_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;

        let condition = self.parse_logic_expression()?;

        self.expect(&Token::Colon)?;

        let then_block = self.parse_block_or_expr()?;

        while self.check(&Token::Newline) {
            self.advance();
        }

        let else_block = if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::Colon)?;
            Some(self.parse_block_or_expr()?)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::If {
            condition,
            then_block,
            else_block,
            span,
        })
    }

    pub(super) fn parse_block_or_expr(&mut self) -> Result<BlockOrExpr, ParseError> {
        self.skip_whitespace();

        if let Some(Token::Identifier(name)) = self.current().map(|t| &t.token) {
            if name == "pass" {
                let span = self.current_span();
                self.advance();
                let _ = self.consume_statement_end();
                return Ok(BlockOrExpr::Pass(span));
            }
        }

        if self.check(&Token::Indent) {
            self.advance();

            let mut statements = Vec::new();

            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                statements.push(self.parse_logic_statement()?);
            }

            self.expect(&Token::Dedent)?;

            if statements.len() == 1 {
                match &statements[0] {
                    LogicStatement::Expression(expr) => {
                        return Ok(BlockOrExpr::Expression(expr.clone()));
                    }
                    LogicStatement::If {
                        condition,
                        then_block,
                        else_block,
                        span,
                    } => {
                        let else_expr = else_block.clone().unwrap_or(BlockOrExpr::Pass(*span));

                        let expr = LogicExpression::If {
                            condition: Box::new(condition.clone()),
                            then_expr: Box::new(then_block.clone()),
                            else_expr: Box::new(else_expr),
                            span: *span,
                        };
                        return Ok(BlockOrExpr::Expression(expr));
                    }
                    _ => {}
                }
            }

            return Ok(BlockOrExpr::Block(statements));
        }

        let expr = self.parse_logic_expression()?;
        Ok(BlockOrExpr::Expression(expr))
    }
}
