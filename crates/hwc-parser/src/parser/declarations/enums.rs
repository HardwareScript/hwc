use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a top-level enum declaration: `enum Name { Variant1, Variant2(Type), ... }`
    pub fn parse_enum_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<EnumDecl, ParseError> {
        self.expect_token(&Token::Enum, "Expected 'enum'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for enum body")?;
        let mut variants = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let var_start = self.current_span().start;
            let var_ident = self.expect_identifier()?;
            let var_name: CompactString = var_ident.name.as_str().into();

            let (payload, var_end) = if self.check(&Token::OpenParen) {
                self.advance();
                let mut tuple_types = Vec::new();
                while !self.check(&Token::CloseParen) && !self.is_at_end() {
                    tuple_types.push(self.parse_type_expr()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let cp = self.expect_token(&Token::CloseParen, "Expected ')'")?;
                (Some(EnumVariantPayload::Tuple(tuple_types)), cp.end)
            } else {
                (None, self.previous_span().end)
            };

            variants.push(EnumVariantDecl {
                name: var_name,
                payload,
                span: Span::new(var_start, var_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close enum body")?;

        Ok(EnumDecl {
            is_exported,
            name,
            variants,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
