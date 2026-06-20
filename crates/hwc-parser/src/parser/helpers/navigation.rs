use crate::lexer::{SpannedToken, Token};
use crate::parser::error::span_to_source_span;
use crate::parser::{ParseError, Parser};
use miette::SourceSpan;

impl Parser {
    /// Get the current token without consuming it
    ///
    /// NOTE: This automatically skips comment tokens, making them invisible to all parsing logic.
    /// Comments are handled at the lexer/parser boundary, not in individual parsing functions.
    pub(crate) fn current(&self) -> Option<&SpannedToken> {
        let mut pos = self.current;

        // Skip over comment tokens to find the next real token
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
    pub(crate) fn peek_ahead(&self, offset: usize) -> Option<&SpannedToken> {
        let mut pos = self.current;
        let mut real_tokens_seen = 0;

        // Skip comments and count real tokens
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
    ///
    /// This is the ONLY method that should modify self.current.
    /// Comments are transparently skipped, making them invisible to all parsing logic.
    pub(crate) fn advance(&mut self) {
        if self.current < self.tokens.len() {
            self.current += 1;

            // Skip over any comment tokens
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

    /// Check if the current token is a specific identifier (keyword-aware)
    pub(crate) fn check_identifier(&self, expected: &str) -> bool {
        if let Some(current) = self.current() {
            let name = match &current.token {
                Token::Identifier(name) => Some(name.as_str()),
                Token::Module => Some("module"),
                Token::Component => Some("component"),
                Token::Space => Some("space"),
                Token::Profile => Some("profile"),
                Token::Material => Some("material"),
                Token::Spanning => Some("spanning"),
                Token::Interface => Some("interface"),
                Token::Device => Some("device"),
                Token::On => Some("on"),
                Token::At => Some("at"),
                Token::To => Some("to"),
                Token::By => Some("by"),
                Token::From => Some("from"),
                Token::Named => Some("named"),
                Token::Dimensions => Some("dimensions"),
                Token::Grid => Some("grid"),
                Token::Path => Some("path"),
                Token::Origin => Some("origin"),
                Token::Let => Some("let"),
                Token::Mut => Some("mut"),
                Token::Const => Some("const"),
                Token::True => Some("true"),
                Token::False => Some("false"),
                Token::Add => Some("add"),
                Token::Route => Some("route"),
                Token::Expose => Some("expose"),
                Token::Rotated => Some("rotated"),
                Token::For => Some("for"),
                Token::In => Some("in"),
                Token::If => Some("if"),
                Token::Else => Some("else"),
                Token::Mod => Some("mod"),
                Token::Implements => Some("implements"),
                Token::Bridge => Some("bridge"),
                Token::Exit => Some("exit"),
                Token::Enter => Some("enter"),
                _ => None,
            };

            if let Some(n) = name {
                return n == expected;
            }
        }
        false
    }

    /// Check if the current token is any identifier or keyword
    pub(crate) fn is_identifier_or_keyword(&self) -> bool {
        if let Some(current) = self.current() {
            matches!(
                current.token,
                Token::Identifier(_)
                    | Token::Module
                    | Token::Component
                    | Token::Space
                    | Token::Profile
                    | Token::Material
                    | Token::Spanning
                    | Token::Interface
                    | Token::Device
                    | Token::On
                    | Token::At
                    | Token::To
                    | Token::By
                    | Token::From
                    | Token::Named
                    | Token::Dimensions
                    | Token::Grid
                    | Token::Path
                    | Token::Origin
                    | Token::Let
                    | Token::Mut
                    | Token::Const
                    | Token::True
                    | Token::False
                    | Token::Add
                    | Token::Route
                    | Token::Expose
                    | Token::Rotated
                    | Token::Implements
                    | Token::Bridge
                    | Token::Exit
                    | Token::Enter
            )
        } else {
            false
        }
    }

    /// Check if current token matches the given token type
    pub(crate) fn check(&self, token: &Token) -> bool {
        if let Some(current) = self.current() {
            &current.token == token
        } else {
            false
        }
    }

    /// Consume the current token if it matches, otherwise return error
    pub(crate) fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        if let Some(current) = self.current() {
            if self.check(expected) {
                let token = current.clone();
                self.advance();
                Ok(token)
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: format!("{expected}").into(),
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

    /// Check if we're at the end of the token stream
    pub(crate) fn is_at_end(&self) -> bool {
        if let Some(current) = self.current() {
            matches!(current.token, Token::Eof)
        } else {
            true
        }
    }

    /// Check if the current minus sign is a binary operator (not unary)
    /// Returns true if the previous token is an atom (identifier, number, closing paren/bracket)
    pub(crate) fn is_binary_minus(&self) -> bool {
        if self.current == 0 {
            return false;
        }

        // Look at the previous non-comment token
        let mut pos = self.current;
        while pos > 0 {
            pos -= 1;
            match &self.tokens[pos].token {
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    continue;
                }
                _ => break,
            }
        }

        if pos >= self.tokens.len() {
            return false;
        }

        matches!(
            &self.tokens[pos].token,
            Token::Identifier(_)
                | Token::Integer(_)
                | Token::Float(_)
                | Token::Measurement(_)
                | Token::CloseParen
                | Token::CloseBracket
                | Token::Rotated
                | Token::At
        )
    }
}
