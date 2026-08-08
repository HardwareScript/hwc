use crate::lexer::Token;
use crate::parser::error::ParseError;
use compact_str::CompactString;

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse net bindings for component pins (v0.1.6 Item #13)
    /// Syntax: [pin1: NetName1, pin2: NetName2, ...]
    /// Supports:
    /// - Simple bindings: a: A[i]
    /// - Conditional bindings: carry_in: if i == 0 then CarryIn else Carry[i-1]
    pub(in crate::parser) fn parse_net_bindings(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, crate::ast::NetBinding>, ParseError> {
        self.expect(&Token::OpenBracket)?;

        let mut bindings = rustc_hash::FxHashMap::default();

        // Parse first binding
        if !self.check(&Token::CloseBracket) {
            let (pin, net) = self.parse_net_binding()?;
            bindings.insert(pin, net);

            // Parse additional bindings separated by commas
            while self.check(&Token::Comma) {
                self.advance();
                // Allow trailing comma
                if self.check(&Token::CloseBracket) {
                    break;
                }
                let (pin, net) = self.parse_net_binding()?;
                bindings.insert(pin, net);
            }
        }

        self.expect(&Token::CloseBracket)?;

        Ok(bindings)
    }

    /// Parse a single net binding: pin: NetName or pin: if condition then Net1 else Net2
    fn parse_net_binding(&mut self) -> Result<(CompactString, crate::ast::NetBinding), ParseError> {
        let pin_name = self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;

        // Check for conditional binding: if condition then Net1 else Net2
        if self.check(&Token::If) {
            self.advance(); // consume 'if'

            // Parse condition expression
            let condition = self.parse_expression()?;

            self.expect(&Token::Then)?;

            // Parse then net name (can be a simple identifier or indexed: A[i])
            let then_net = self.parse_net_name_string()?;

            self.expect(&Token::Else)?;

            // Parse else net name
            let else_net = self.parse_net_name_string()?;

            Ok((
                pin_name.into(),
                crate::ast::NetBinding::Conditional {
                    condition,
                    then_net: then_net.into(),
                    else_net: else_net.into(),
                },
            ))
        } else {
            // Simple binding: pin: NetName or pin: Net[i]
            let net_name = self.parse_net_name_string()?;
            Ok((
                pin_name.into(),
                crate::ast::NetBinding::Simple(net_name.into()),
            ))
        }
    }

    /// Parse a net name string with optional array indexing: NetName or Net[i] or Net[i-1]
    /// This is different from parse_net_name in helpers.rs which returns a NetName AST node
    fn parse_net_name_string(&mut self) -> Result<String, ParseError> {
        let base_name = self.expect_identifier_string()?;

        // Check for array syntax: Name[expr]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['

            // Parse the index expression as a string (we'll evaluate it later)
            let mut index_str = String::new();
            let mut bracket_depth = 1;

            while bracket_depth > 0 && !self.is_at_end() {
                if let Some(spanned_token) = self.current() {
                    match &spanned_token.token {
                        Token::OpenBracket => {
                            index_str.push('[');
                            bracket_depth += 1;
                            self.advance();
                        }
                        Token::CloseBracket => {
                            bracket_depth -= 1;
                            if bracket_depth > 0 {
                                index_str.push(']');
                            }
                            self.advance();
                        }
                        Token::Identifier(name) => {
                            index_str.push_str(name);
                            self.advance();
                        }
                        Token::Integer(n) => {
                            index_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Plus => {
                            index_str.push('+');
                            self.advance();
                        }
                        Token::Hyphen => {
                            index_str.push('-');
                            self.advance();
                        }
                        Token::Asterisk => {
                            index_str.push('*');
                            self.advance();
                        }
                        Token::Slash => {
                            index_str.push('/');
                            self.advance();
                        }
                        _ => {
                            return Err(self.error(&format!(
                                "Unexpected token in net name index: {}",
                                spanned_token.token
                            )));
                        }
                    }
                } else {
                    break;
                }
            }

            if bracket_depth != 0 {
                return Err(self.error("Unclosed bracket in net name index"));
            }

            Ok(format!("{}[{}]", base_name, index_str))
        } else {
            Ok(base_name)
        }
    }
}
