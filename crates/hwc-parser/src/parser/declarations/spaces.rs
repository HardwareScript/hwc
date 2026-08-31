use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a space declaration: `(#[attr])* space Name implements Interface { ... }`
    pub fn parse_space_decl(&mut self, mut attributes: Vec<Attribute>, start_pos: usize) -> Result<SpaceDecl, ParseError> {
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        self.expect_token(&Token::Space, "Expected 'space'")?;
        let name = self.expect_identifier()?;

        let implements = if self.check(&Token::Implements) {
            self.advance();
            Some(self.expect_identifier()?)
        } else {
            None
        };

        self.expect_token(&Token::OpenBrace, "Expected '{' for space body")?;

        let mut dimensions = None;
        let mut profile = None;
        let mut nets = Vec::new();
        let mut statements = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            if self.check_identifier("nets") {
                // `nets { VDD: { ... }, VSS: { ... } }`
                self.advance(); // consume `nets`
                self.expect_token(&Token::OpenBrace, "Expected '{' after nets")?;

                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    let net_start = self.current_span().start;
                    let net_ident = self.expect_identifier()?;
                    let net_name: CompactString = net_ident.name.as_str().into();

                    self.expect_token(&Token::Colon, "Expected ':' after net name")?;
                    self.expect_token(&Token::OpenBrace, "Expected '{' for net properties")?;

                    let mut properties = Vec::new();
                    while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                        let prop_ident = self.expect_identifier()?;
                        let prop_name: CompactString = prop_ident.name.as_str().into();
                        self.expect_token(&Token::Colon, "Expected ':' after property name")?;
                        let prop_val = self.parse_expression()?;
                        properties.push((prop_name, prop_val));

                        if self.check(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }

                    let close_net = self.expect_token(&Token::CloseBrace, "Expected '}' to close net properties")?;
                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }

                    nets.push(NetDecl {
                        name: net_name,
                        properties,
                        span: Span::new(net_start, close_net.end),
                    });
                }

                self.expect_token(&Token::CloseBrace, "Expected '}' to close nets block")?;
                continue;
            }

            let is_dimensions = match self.current().map(|t| &t.token) {
                Some(Token::Identifier(s)) => s == "dimensions",
                _ => false,
            };

            if is_dimensions {
                self.advance();
                self.expect_token(&Token::Colon, "Expected ':' after dimensions")?;
                self.expect_token(&Token::OpenBracket, "Expected '[' for dimensions [width, height]")?;
                let width = self.parse_expression()?;
                self.expect_token(&Token::Comma, "Expected ',' between dimensions")?;
                let height = self.parse_expression()?;
                self.expect_token(&Token::CloseBracket, "Expected ']' to close dimensions")?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                dimensions = Some((width, height));
                continue;
            }

            let is_profile = match self.current().map(|t| &t.token) {
                Some(Token::Profile) => true,
                Some(Token::Identifier(s)) => s == "profile",
                _ => false,
            };

            if is_profile {
                self.advance();
                self.expect_token(&Token::Colon, "Expected ':' after profile")?;
                let prof_ident = self.expect_identifier()?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                profile = Some(prof_ident);
                continue;
            }

            // Otherwise, it is a statement (e.g. let, region, function call, route, etc.)
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close space body")?;

        Ok(SpaceDecl {
            attributes,
            name,
            implements,
            dimensions,
            profile,
            nets,
            statements,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
