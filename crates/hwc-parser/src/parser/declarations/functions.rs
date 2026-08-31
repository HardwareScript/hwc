use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a top-level function declaration
    pub fn parse_function_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<FunctionDecl, ParseError> {
        self.expect_token(&Token::Fn, "Expected 'fn'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenParen, "Expected '(' for function parameter list")?;
        let mut parameters = Vec::new();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            let param_start = self.current_span().start;
            let param_ident = self.expect_identifier()?;
            let param_name: CompactString = param_ident.name.as_str().into();

            let type_annotation = if param_name == "self" && !self.check(&Token::Colon) {
                TypeExpr::Named {
                    name: "Self".into(),
                    type_args: Vec::new(),
                    span: param_ident.span,
                }
            } else {
                self.expect_token(&Token::Colon, "Expected ':' after parameter name")?;
                self.parse_type_expr()?
            };

            let default_value = if self.check(&Token::Equals) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };

            let param_end = if let Some(def) = &default_value {
                def.span().end
            } else {
                type_annotation.span().end
            };

            parameters.push(Parameter {
                name: param_name,
                type_annotation,
                default_value,
                span: Span::new(param_start, param_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect_token(&Token::CloseParen, "Expected ')' to close parameter list")?;

        let return_type = if self.check(&Token::Arrow) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        let end_pos = body.span.end;

        Ok(FunctionDecl {
            is_exported,
            name,
            parameters,
            return_type,
            body,
            span: Span::new(start_pos, end_pos),
        })
    }
}
