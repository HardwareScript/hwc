use crate::lexer::{Span, Token};
use crate::parser::error::span_to_source_span;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;
use miette::SourceSpan;

impl Parser {
    /// Create an error at the current position
    pub(crate) fn error(&self, message: &str) -> ParseError {
        if let Some(current) = self.current() {
            ParseError::General {
                span: span_to_source_span(&current.span),
                message: message.into(),
            }
        } else {
            // For EOF, use the last token's span or a zero-length span at the end
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            ParseError::UnexpectedEof { span }
        }
    }

    /// Get the span of the current token
    pub(crate) fn current_span(&self) -> Span {
        if let Some(token) = self.current() {
            token.span
        } else {
            // Return a zero-width span at the end
            Span::new(0, 0)
        }
    }

    /// Get the span of the previous token
    pub(crate) fn previous_span(&self) -> Span {
        if self.current > 0 {
            self.tokens[self.current - 1].span
        } else {
            Span::new(0, 0)
        }
    }

    /// Get a human-readable description of a token for error messages
    pub(crate) fn token_description(&self, token: &Token) -> CompactString {
        match token {
            Token::Integer(n) => format!("integer '{}'", n).into(),
            Token::Float(f) => format!("float '{}'", f).into(),
            Token::String(s) => format!("string \"{}\"", s).into(),
            Token::Identifier(id) => format!("identifier '{}'", id).into(),
            Token::Measurement(m) => format!("measurement '{:?}'", m).into(),
            Token::Plus => "operator '+'".into(),
            Token::Hyphen => "operator '-'".into(),
            Token::Asterisk => "operator '*'".into(),
            Token::Slash => "operator '/'".into(),
            Token::Percent => "operator '%'".into(),
            Token::OpenParen => "'('".into(),
            Token::CloseParen => "')'".into(),
            Token::OpenBracket => "'['".into(),
            Token::CloseBracket => "']'".into(),
            Token::Comma => "','".into(),
            Token::Colon => "':'".into(),
            Token::Dot => "'.'".into(),
            Token::Newline => "newline".into(),
            _ => format!("{:?}", token).into(),
        }
    }
}
