use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a material declaration: `material Name { prop: val, ... }`
    pub fn parse_material_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<MaterialDecl, ParseError> {
        self.expect_token(&Token::Material, "Expected 'material'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for material body")?;
        let mut properties = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let prop_ident = self.expect_identifier()?;
            let prop_name: CompactString = prop_ident.name.as_str().into();
            self.expect_token(&Token::Colon, "Expected ':' after material property name")?;
            let prop_val = self.parse_expression()?;
            properties.push((prop_name, prop_val));

            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close material body")?;

        Ok(MaterialDecl {
            is_exported,
            name,
            properties,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
