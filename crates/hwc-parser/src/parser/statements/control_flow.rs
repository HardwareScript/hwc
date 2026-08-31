use crate::ast::{ElseBranch, MatchArm, Pattern, Statement};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    pub(super) fn parse_break_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Break, "Expected 'break'")?;
        let end_pos = self.previous_span().end;
        if self.check(&Token::Semicolon) {
            self.advance();
        }
        Ok(Statement::Break {
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    pub(super) fn parse_continue_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Continue, "Expected 'continue'")?;
        let end_pos = self.previous_span().end;
        if self.check(&Token::Semicolon) {
            self.advance();
        }
        Ok(Statement::Continue {
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    pub(super) fn parse_if_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::If, "Expected 'if'")?;
        let condition = self.parse_expression()?;
        let then_block = self.parse_block()?;

        let else_branch = if self.check(&Token::Else) {
            self.advance();
            if self.check(&Token::If) {
                let else_if = self.parse_if_statement(self.current_span().start)?;
                Some(ElseBranch::ElseIf(Box::new(else_if)))
            } else {
                let else_block = self.parse_block()?;
                Some(ElseBranch::Block(else_block))
            }
        } else {
            None
        };

        let end_pos = match &else_branch {
            Some(ElseBranch::ElseIf(stmt)) => stmt.span().end,
            Some(ElseBranch::Block(blk)) => blk.span.end,
            None => then_block.span.end,
        };

        Ok(Statement::If {
            condition,
            then_block,
            else_branch,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    pub(super) fn parse_for_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::For, "Expected 'for'")?;

        let first_ident = self.expect_identifier()?;
        let mut variables = vec![CompactString::from(first_ident.name.as_str())];

        if self.check(&Token::Comma) {
            self.advance();
            let second_ident = self.expect_identifier()?;
            variables.push(CompactString::from(second_ident.name.as_str()));
        }

        self.expect_token(&Token::In, "Expected 'in' in for loop")?;
        let iterable = self.parse_expression()?;

        let key = if self.check(&Token::Key) {
            self.advance();
            self.expect_token(&Token::Colon, "Expected ':' after 'key'")?;
            let key_expr = self.parse_expression()?;
            Some(key_expr)
        } else if let Some(crate::lexer::Token::Identifier(s)) = self.current().map(|t| &t.token) {
            if s == "key" {
                self.advance();
                self.expect_token(&Token::Colon, "Expected ':' after 'key'")?;
                let key_expr = self.parse_expression()?;
                Some(key_expr)
            } else {
                None
            }
        } else {
            None
        };

        let body = self.parse_block()?;
        let end_pos = body.span.end;

        Ok(Statement::For {
            variables,
            iterable,
            key,
            body,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    pub(super) fn parse_match_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Match, "Expected 'match'")?;
        let target = self.parse_expression()?;
        self.expect_token(&Token::OpenBrace, "Expected '{' after match target")?;

        let mut arms = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let arm_start = self.current_span().start;

            let pattern = if self.check(&Token::Underscore) {
                let span = self.current_span();
                self.advance();
                Pattern::Wildcard { span }
            } else if let Some(crate::lexer::Token::Identifier(id)) = self.current().map(|t| &t.token) {
                if id.as_str() == "_" {
                    let span = self.current_span();
                    self.advance();
                    Pattern::Wildcard { span }
                } else {
                    Pattern::Expr(self.parse_expression()?)
                }
            } else {
                Pattern::Expr(self.parse_expression()?)
            };

            self.expect_token(&Token::FatArrow, "Expected '=>' after match pattern")?;
            let body = self.parse_block()?;
            let arm_end = body.span.end;

            if self.check(&Token::Comma) {
                self.advance();
            }

            arms.push(MatchArm {
                pattern,
                body,
                span: crate::ast::Span::new(arm_start, arm_end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close match")?;

        Ok(Statement::Match {
            target,
            arms,
            span: crate::ast::Span::new(start_pos, close_span.end),
        })
    }

    pub(super) fn parse_return_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Return, "Expected 'return'")?;

        let (value, end_pos) = if !self.check(&Token::Semicolon)
            && !self.check(&Token::CloseBrace)
            && !self.is_at_end()
        {
            let expr = self.parse_expression()?;
            let end = expr.span().end;
            (Some(expr), end)
        } else {
            (None, self.previous_span().end)
        };

        Ok(Statement::Return {
            value,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }
}
