use crate::ast::{BindingPattern, Block, Statement};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    pub(super) fn parse_route_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Route, "Expected 'route'")?;
        let from = self.parse_expression()?;
        self.expect_token(&Token::To, "Expected 'to' in route statement")?;
        let to = self.parse_expression()?;

        let mut intent = None;
        let mut body = None;

        if self.check_identifier("with") {
            self.advance();
            if !self.check_identifier("intent") {
                return Err(ParseError::UnexpectedToken {
                    span: crate::parser::error::span_to_source_span(&self.current_span()),
                    expected: "'intent'".into(),
                    found: self.current().map(|t| format!("{}", t.token)).unwrap_or_default().into(),
                });
            }
            self.advance();
            self.expect_token(&Token::Colon, "Expected ':' after intent")?;
            let intent_ident = self.expect_identifier()?;
            intent = Some(intent_ident.name.as_str().into());
        }

        if self.check(&Token::OpenBrace) {
            let brace_start = self.current_span().start;
            self.advance();
            let mut stmts = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                while self.check(&Token::Semicolon) || self.check(&Token::Comma) {
                    self.advance();
                }
                if self.check(&Token::CloseBrace) || self.is_at_end() {
                    break;
                }
                let prop_start = self.current_span().start;
                let is_key_colon = if let Some(Token::Identifier(_)) = self.current().map(|t| &t.token) {
                    self.peek_ahead(1).map(|t| &t.token) == Some(&Token::Colon)
                } else {
                    false
                };

                if is_key_colon {
                    let key_ident = self.expect_identifier()?;
                    self.expect_token(&Token::Colon, "Expected ':' after route property key")?;
                    let val_expr = self.parse_expression()?;
                    let prop_end = val_expr.span().end;
                    stmts.push(Statement::Let {
                        mutable: false,
                        pattern: BindingPattern::Identifier(key_ident.name.as_str().into()),
                        type_annotation: None,
                        value: val_expr,
                        span: crate::ast::Span::new(prop_start, prop_end),
                    });
                    if self.check(&Token::Comma) || self.check(&Token::Semicolon) {
                        self.advance();
                    }
                } else {
                    let s = self.parse_statement()?;
                    stmts.push(s);
                    if self.check(&Token::Comma) || self.check(&Token::Semicolon) {
                        self.advance();
                    }
                }
            }
            let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close route properties")?;
            body = Some(Block {
                statements: stmts,
                tail_expr: None,
                span: crate::ast::Span::new(brace_start, close_span.end),
            });
        }

        let end_pos = if let Some(blk) = &body {
            blk.span.end
        } else {
            to.span().end
        };

        Ok(Statement::Route {
            from,
            to,
            intent,
            body,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }
}
