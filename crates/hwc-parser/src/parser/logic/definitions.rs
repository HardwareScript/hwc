use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'ast> Parser<'ast> {
    pub fn parse_logic_definition(
        &mut self,
        is_exported: bool,
    ) -> Result<LogicDefinition, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Logic)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut statements = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            statements.push(self.parse_logic_statement()?);
        }

        self.expect(&Token::Dedent)?;

        let logic_span = Span::new(start.start, self.previous_span().end);
        let logic_block = LogicBlock {
            statements,
            span: logic_span,
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicDefinition {
            name,
            is_exported,
            logic_block,
            span,
        })
    }

    pub fn parse_enum(&mut self, is_exported: bool) -> Result<EnumDefinition, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Enum)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut variants = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let variant_start = self.current_span();
            let variant_name = self.expect_identifier_string()?;

            let value = if self.check(&Token::Equals) {
                self.advance();
                if let Some(Token::Integer(n)) = self.current().map(|t| &t.token) {
                    let val = *n;
                    self.advance();
                    Some(val)
                } else {
                    return Err(self.error("Expected integer value after '=' in enum variant"));
                }
            } else {
                None
            };

            if self.check(&Token::Comma) {
                self.advance();
            }

            self.consume_statement_end()?;

            let variant_span = Span::new(variant_start.start, self.previous_span().end);

            variants.push(EnumVariant {
                name: variant_name.into(),
                value,
                span: variant_span,
            });
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(EnumDefinition {
            name,
            is_exported,
            variants,
            span,
        })
    }

    pub fn parse_struct(&mut self, is_exported: bool) -> Result<StructDefinition, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Struct)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut fields = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_start = self.current_span();
            let field_name = self.expect_identifier_string()?;

            self.expect(&Token::OpenBracket)?;
            let width = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;

            self.consume_statement_end()?;

            let field_span = Span::new(field_start.start, self.previous_span().end);

            fields.push(StructField {
                name: field_name.into(),
                width,
                span: field_span,
            });
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(StructDefinition {
            name,
            is_exported,
            fields,
            span,
        })
    }
}
