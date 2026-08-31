use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse a module declaration: `(#[attr])* module Name { pins: [input In, output Out], logic { ... }, ... }`
    pub fn parse_module_decl(&mut self, mut attributes: Vec<Attribute>, start_pos: usize) -> Result<ModuleDecl, ParseError> {
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        self.expect_token(&Token::Module, "Expected 'module'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for module body")?;
        let mut pins = Vec::new();
        let mut logic_blocks = Vec::new();
        let mut routes = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            if self.check_identifier("pins") {
                self.advance(); // consume `pins`
                self.expect_token(&Token::Colon, "Expected ':' after pins")?;
                self.expect_token(&Token::OpenBracket, "Expected '[' for pin list")?;

                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    let pin_start = self.current_span().start;
                    let first_ident = self.expect_identifier()?;

                    // Check if first ident was direction (input, output, inout, power, ground)
                    let (dir, pin_name) = match first_ident.name.as_str() {
                        "input" | "output" | "inout" | "power" | "ground" => {
                            let second_ident = self.expect_identifier()?;
                            (Some(first_ident.name.as_str().into()), second_ident.name.as_str().into())
                        }
                        other => (None, other.into()),
                    };

                    let pin_end = self.previous_span().end;
                    pins.push(PinDecl {
                        direction: dir,
                        name: pin_name,
                        span: Span::new(pin_start, pin_end),
                    });

                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }

                self.expect_token(&Token::CloseBracket, "Expected ']' to close pin list")?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                continue;
            }

            if self.check(&Token::Logic) {
                let logic_start = self.current_span().start;
                let logic_blk = self.parse_logic_block(logic_start)?;
                logic_blocks.push(logic_blk);
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                continue;
            }

            // Route statement or other statement inside module
            let stmt = self.parse_statement()?;
            routes.push(stmt);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close module body")?;

        Ok(ModuleDecl {
            attributes,
            name,
            pins,
            logic_blocks,
            routes,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
