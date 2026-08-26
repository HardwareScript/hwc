//! Lexer implementation for HardwareScript v0.3.0 (Zero Indentation Tracking)

use logos::Logos;
use super::error::{span_to_source_span, LexError};
use super::span::{Span, SpannedToken};
use super::token::Token;

/// Lexer for HardwareScript v0.3.0
pub struct Lexer<'source> {
    source: &'source str,
    tokens: Vec<SpannedToken>,
}

impl<'source> Lexer<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            tokens: Vec::new(),
        }
    }

    /// Tokenize the source code into a stream of SpannedTokens
    pub fn tokenize(mut self) -> Result<Vec<SpannedToken>, LexError> {
        let mut lexer = Token::lexer(self.source);

        while let Some(token_result) = lexer.next() {
            let span = lexer.span();

            match token_result {
                Ok(token) => {
                    self.tokens.push(SpannedToken::new(
                        token,
                        Span::new(span.start, span.end),
                    ));
                }
                Err(_) => {
                    return Err(LexError::InvalidToken {
                        span: span_to_source_span(&Span::new(span.start, span.end)),
                        text: self.source[span.start..span.end].to_string().into(),
                    });
                }
            }
        }

        // Emit EOF token at the end of input
        self.tokens.push(SpannedToken::new(
            Token::Eof,
            Span::new(self.source.len(), self.source.len()),
        ));

        Ok(self.tokens)
    }
}
