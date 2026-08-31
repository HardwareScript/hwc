use crate::ast::TypeExpr;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    pub(crate) fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let start_pos = self.current_span().start;

        if self.check(&Token::Fn) {
            self.advance();
            self.expect_token(&Token::OpenParen, "Expected '(' for function type parameters")?;
            let mut params = Vec::new();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                params.push(self.parse_type_expr()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseParen, "Expected ')'")?;

            let return_type = if self.check(&Token::Arrow) {
                self.advance();
                Some(Box::new(self.parse_type_expr()?))
            } else {
                None
            };

            let end_pos = if let Some(ret) = &return_type {
                ret.span().end
            } else {
                self.previous_span().end
            };

            Ok(TypeExpr::Function {
                params,
                return_type,
                span: crate::ast::Span::new(start_pos, end_pos),
            })
        } else if self.check(&Token::OpenParen) {
            self.advance();
            let mut elements = Vec::new();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                elements.push(self.parse_type_expr()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close tuple type")?;
            Ok(TypeExpr::Tuple {
                elements,
                span: crate::ast::Span::new(start_pos, close_span.end),
            })
        } else {
            let ident = self.expect_identifier()?;
            let type_name: CompactString = ident.name.as_str().into();
            let mut type_args = Vec::new();

            if self.check(&Token::OpenBracket) {
                self.advance();
                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    type_args.push(self.parse_type_expr()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close generic type arguments")?;
                Ok(TypeExpr::Named {
                    name: type_name,
                    type_args,
                    span: crate::ast::Span::new(start_pos, close_span.end),
                })
            } else {
                Ok(TypeExpr::Named {
                    name: type_name,
                    type_args: Vec::new(),
                    span: crate::ast::Span::new(start_pos, self.previous_span().end),
                })
            }
        }
    }
}
