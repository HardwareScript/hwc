use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a test declaration: `test Name for TargetSpace { dc: { ... }, tran: { ... } }`
    pub fn parse_test_decl(&mut self, start_pos: usize) -> Result<TestDecl, ParseError> {
        self.expect_token(&Token::Test, "Expected 'test'")?;
        let name = self.expect_identifier()?;
        self.expect_token(&Token::For, "Expected 'for' after test name")?;
        let target = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for test body")?;
        let mut configs = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let cfg_start = self.current_span().start;
            let cfg_ident = self.expect_identifier()?;
            let cfg_name: CompactString = cfg_ident.name.as_str().into();

            self.expect_token(&Token::Colon, "Expected ':' after test config name")?;
            self.expect_token(&Token::OpenBrace, "Expected '{' for test config parameters")?;

            let mut params = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                let p_ident = self.expect_identifier()?;
                let p_name: CompactString = p_ident.name.as_str().into();
                self.expect_token(&Token::Colon, "Expected ':' after parameter name")?;
                let p_val = self.parse_expression()?;
                params.push((p_name, p_val));

                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }

            let close_cfg = self.expect_token(&Token::CloseBrace, "Expected '}' to close test config")?;
            if self.check(&Token::Semicolon) {
                self.advance();
            }

            configs.push(TestConfig {
                name: cfg_name,
                params,
                span: Span::new(cfg_start, close_cfg.end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close test body")?;

        Ok(TestDecl {
            name,
            target,
            configs,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
