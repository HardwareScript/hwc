use crate::ast::Identifier;
use crate::lexer::Token;
use crate::parser::error::{span_to_source_span, ParseError};
use crate::parser::Parser;
use miette::SourceSpan;

impl Parser {
    /// Expect and consume an identifier token, returning Identifier with span.
    pub fn expect_identifier(&mut self) -> Result<Identifier, ParseError> {
        if let Some(current) = self.current() {
            let identifier_name = match &current.token {
                Token::Identifier(name) => Some(name.clone().into()),
                Token::Space => Some("space".into()),
                Token::Module => Some("module".into()),
                Token::Device => Some("device".into()),
                Token::Material => Some("material".into()),
                Token::Profile => Some("profile".into()),
                Token::Route => Some("route".into()),
                Token::Test => Some("test".into()),
                Token::Nets => Some("nets".into()),
                Token::Pins => Some("pins".into()),
                Token::Let => Some("let".into()),
                Token::Mut => Some("mut".into()),
                Token::Const => Some("const".into()),
                Token::True => Some("true".into()),
                Token::False => Some("false".into()),
                Token::Fn => Some("fn".into()),
                Token::Struct => Some("struct".into()),
                Token::Enum => Some("enum".into()),
                Token::If => Some("if".into()),
                Token::Else => Some("else".into()),
                Token::For => Some("for".into()),
                Token::In => Some("in".into()),
                Token::Return => Some("return".into()),
                Token::Assert => Some("assert".into()),
                Token::Match => Some("match".into()),
                Token::Import => Some("import".into()),
                Token::Export => Some("export".into()),
                Token::From => Some("from".into()),
                Token::Implements => Some("implements".into()),
                Token::To => Some("to".into()),
                Token::With => Some("with".into()),
                Token::Intent => Some("intent".into()),
                _ => None,
            };

            if let Some(name) = identifier_name {
                let identifier = Identifier::new(name, current.span);
                self.advance();
                Ok(identifier)
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "identifier".into(),
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

    /// Expect and consume a string token
    pub fn expect_string(&mut self) -> Result<String, ParseError> {
        if let Some(current) = self.current() {
            if let Token::String(s) = &current.token {
                let result = s.clone();
                self.advance();
                Ok(result)
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "string".into(),
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
}
