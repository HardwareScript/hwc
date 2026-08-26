use crate::lexer::{Span, SpannedToken, Token};
use crate::parser::error::span_to_source_span;
use crate::parser::{ParseError, Parser};
use miette::SourceSpan;

impl Parser {
    /// Get the current token without consuming it (skips comments)
    pub fn current(&self) -> Option<&SpannedToken> {
        let mut pos = self.current;

        while pos < self.tokens.len() {
            let token = &self.tokens[pos];
            match token.token {
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => return Some(token),
            }
        }

        None
    }

    /// Peek ahead by offset tokens (skips comments automatically)
    pub fn peek_ahead(&self, offset: usize) -> Option<&SpannedToken> {
        let mut pos = self.current;
        let mut real_tokens_seen = 0;

        while pos < self.tokens.len() {
            let token = &self.tokens[pos];
            match token.token {
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => {
                    if real_tokens_seen == offset {
                        return Some(token);
                    }
                    real_tokens_seen += 1;
                    pos += 1;
                }
            }
        }

        None
    }

    /// Move to the next token (automatically skips comments)
    pub fn advance(&mut self) {
        if self.current < self.tokens.len() {
            self.current += 1;

            while self.current < self.tokens.len() {
                match self.tokens[self.current].token {
                    Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                        self.current += 1;
                    }
                    _ => break,
                }
            }
        }
    }

    /// Check if current token matches the given token type
    pub fn check(&self, token: &Token) -> bool {
        if let Some(current) = self.current() {
            &current.token == token
        } else {
            false
        }
    }

    /// Consume the current token if it matches, otherwise return error
    pub fn expect_token(&mut self, expected: &Token, help_msg: &str) -> Result<Span, ParseError> {
        if let Some(current) = self.current() {
            if self.check(expected) {
                let span = current.span;
                self.advance();
                Ok(span)
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: format!("{expected} ({help_msg})").into(),
                    found: format!("{}", current.token).into(),
                })
            }
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            Err(ParseError::UnexpectedEof { span })
        }
    }

    /// Get the current token span
    pub fn current_span(&self) -> Span {
        if let Some(current) = self.current() {
            current.span
        } else if let Some(last) = self.tokens.last() {
            last.span
        } else {
            Span::new(0, 0)
        }
    }

    /// Get the previous token span
    pub fn previous_span(&self) -> Span {
        if self.current > 0 && self.current <= self.tokens.len() {
            self.tokens[self.current - 1].span
        } else {
            Span::new(0, 0)
        }
    }

    /// Check if we're at the end of the token stream
    pub fn is_at_end(&self) -> bool {
        if let Some(current) = self.current() {
            matches!(current.token, Token::Eof)
        } else {
            true
        }
    }

    /// Create a general parse error
    pub fn error(&self, message: &str) -> ParseError {
        ParseError::General {
            span: span_to_source_span(&self.current_span()),
            message: message.into(),
        }
    }
}
