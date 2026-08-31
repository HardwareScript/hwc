use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a top-level struct declaration: `struct Name { field: Type, ... }`
    pub fn parse_struct_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<StructDecl, ParseError> {
        self.expect_token(&Token::Struct, "Expected 'struct'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for struct body")?;
        let mut fields = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let field_start = self.current_span().start;
            let field_ident = self.expect_identifier()?;
            let field_name: CompactString = field_ident.name.as_str().into();

            self.expect_token(&Token::Colon, "Expected ':' after field name")?;
            let type_annotation = self.parse_type_expr()?;
            let field_end = type_annotation.span().end;

            fields.push(StructFieldDecl {
                name: field_name,
                type_annotation,
                span: Span::new(field_start, field_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close struct body")?;

        Ok(StructDecl {
            is_exported,
            name,
            fields,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
