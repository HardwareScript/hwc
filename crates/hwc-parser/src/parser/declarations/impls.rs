use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse a struct impl block: `impl TargetType { (fn ...)* }`
    pub fn parse_impl_decl(&mut self, start_pos: usize) -> Result<ImplDecl, ParseError> {
        self.expect_token(&Token::Impl, "Expected 'impl'")?;
        let target = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for impl block body")?;
        let mut methods = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let fn_start = self.current_span().start;
            let method = self.parse_function_decl(false, fn_start)?;
            methods.push(method);
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close impl block")?;

        Ok(ImplDecl {
            target,
            methods,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
