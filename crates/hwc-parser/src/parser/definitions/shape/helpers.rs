use crate::lexer::Token;

impl<'ast> crate::parser::Parser<'ast> {
    pub(in crate::parser::definitions::shape) fn read_expression_until_comma_or_close(
        &mut self,
    ) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;
        let mut depth = 0i32;

        while !self.is_at_end() {
            match self.current().map(|t| &t.token) {
                Some(Token::Comma) if depth == 0 => break,
                Some(Token::CloseParen) if depth == 0 => break,
                Some(Token::OpenParen) => {
                    depth += 1;
                }
                Some(Token::CloseParen) => {
                    depth -= 1;
                }
                Some(Token::Newline) | Some(Token::Dedent) => break,
                _ => {}
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    pub(in crate::parser::definitions::shape) fn read_expression_until(
        &mut self,
        delimiter: &Token,
    ) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;

        while !self.is_at_end() {
            if self.check(delimiter) {
                break;
            }
            if self.check(&Token::Newline) || self.check(&Token::Dedent) {
                break;
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    pub(in crate::parser::definitions::shape) fn read_expression_string(
        &mut self,
    ) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;

        while !self.is_at_end() {
            if self.check(&Token::Comma)
                || self.check(&Token::CloseParen)
                || self.check(&Token::Newline)
                || self.check(&Token::Dedent)
            {
                break;
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    fn token_to_string(&self, token: &Token) -> String {
        match token {
            Token::Identifier(s) => s.clone(),
            Token::Integer(n) => n.to_string(),
            Token::Float(n) => n.to_string(),
            Token::Measurement(m) => format!("{}", m),
            Token::Hyphen => "-".to_string(),
            Token::Plus => "+".to_string(),
            Token::Asterisk => "*".to_string(),
            Token::Slash => "/".to_string(),
            Token::Percent => "%".to_string(),
            Token::OpenParen => "(".to_string(),
            Token::CloseParen => ")".to_string(),
            Token::OpenBracket => "[".to_string(),
            Token::CloseBracket => "]".to_string(),
            Token::Dot => ".".to_string(),
            Token::Colon => ":".to_string(),
            Token::Comma => ",".to_string(),
            Token::Equals => "=".to_string(),
            Token::LessThan => "<".to_string(),
            Token::GreaterThan => ">".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Pipe => "|".to_string(),
            Token::Tilde => "~".to_string(),
            Token::Exclamation => "!".to_string(),
            Token::ShiftLeft => "<<".to_string(),
            Token::ShiftRight => ">>".to_string(),
            Token::LessThanOrEqual => "<=".to_string(),
            Token::GreaterThanOrEqual => ">=".to_string(),
            Token::NotEquals => "!=".to_string(),
            Token::Range => "..".to_string(),
            Token::OpenBrace => "{".to_string(),
            Token::CloseBrace => "}".to_string(),
            Token::If => "if".to_string(),
            Token::Else => "else".to_string(),
            Token::Mod => "mod".to_string(),
            Token::For => "for".to_string(),
            Token::In => "in".to_string(),
            Token::Let => "let".to_string(),
            Token::True => "true".to_string(),
            Token::False => "false".to_string(),
            Token::And => "and".to_string(),
            Token::Or => "or".to_string(),
            Token::Not => "not".to_string(),
            _ => format!("<{:?}>", token),
        }
    }
}
