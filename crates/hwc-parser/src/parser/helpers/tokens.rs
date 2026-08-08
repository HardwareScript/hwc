use crate::ast::Identifier;
use crate::lexer::{Span, Token};
use crate::parser::error::{span_to_source_span, ParseError};
use crate::parser::Parser;
use miette::SourceSpan;

impl<'ast> Parser<'ast> {
    /// Expect and consume an identifier token, returning Identifier with span.
    /// v0.1.6: Context-aware parsing allows keywords to be treated as identifiers
    /// when they are in an identifier position (e.g. property names).
    pub(crate) fn expect_identifier(&mut self) -> Result<crate::ast::Identifier, ParseError> {
        if let Some(current) = self.current() {
            // v0.1.6 Migration: Detect quoted string (old v0.1.5 syntax)
            if let Token::String(_) = &current.token {
                return Err(crate::parser::error::error_expected_identifier_not_string(
                    &current.span,
                ));
            }

            // Treat keywords as identifiers in this context
            let identifier_name = match &current.token {
                Token::Identifier(name) => Some(name.clone().into()),
                Token::Module => Some("module".into()),
                Token::Component => Some("component".into()),
                Token::Space => Some("space".into()),
                Token::Profile => Some("profile".into()),
                Token::Material => Some("material".into()),
                Token::Spanning => Some("spanning".into()),
                Token::Interface => Some("interface".into()),
                Token::Device => Some("device".into()),
                Token::SpiceModel => Some("spice_model".into()),
                Token::On => Some("on".into()),
                Token::At => Some("at".into()),
                Token::To => Some("to".into()),
                Token::By => Some("by".into()),
                Token::From => Some("from".into()),
                Token::Named => Some("named".into()),
                Token::Dimensions => Some("dimensions".into()),
                Token::Grid => Some("grid".into()),
                Token::Path => Some("path".into()),
                Token::Origin => Some("origin".into()),
                Token::Let => Some("let".into()),
                Token::Mut => Some("mut".into()),
                Token::Const => Some("const".into()),
                Token::True => Some("true".into()),
                Token::False => Some("false".into()),
                Token::Add => Some("add".into()),
                Token::Route => Some("route".into()),
                Token::Expose => Some("expose".into()),
                Token::Rotated => Some("rotated".into()),
                Token::Implements => Some("implements".into()),
                Token::Bridge => Some("bridge".into()),
                Token::Exit => Some("exit".into()),
                Token::Enter => Some("enter".into()),
                Token::Logic => Some("logic".into()),
                Token::Test => Some("test".into()),
                Token::Enum => Some("enum".into()),
                Token::Struct => Some("struct".into()),
                Token::Unit => Some("unit".into()),
                Token::SignalGroup => Some("signal_group".into()),
                Token::Shape => Some("shape".into()),
                Token::Mechanical => Some("mechanical".into()),
                Token::Substrate => Some("substrate".into()),
                Token::Plane => Some("plane".into()),
                Token::Resolution => Some("resolution".into()),
                Token::For => Some("for".into()),
                Token::In => Some("in".into()),
                Token::If => Some("if".into()),
                Token::Then => Some("then".into()),
                Token::Else => Some("else".into()),
                Token::Align => Some("align".into()),
                Token::With => Some("with".into()),
                Token::Inside => Some("inside".into()),
                Token::Region => Some("region".into()),
                Token::Subcircuit => Some("subcircuit".into()),
                _ => None,
            };

            if let Some(name) = identifier_name {
                let identifier = crate::ast::Identifier::new(name, current.span);
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

    /// Expect and consume an identifier with a specific name
    pub(crate) fn expect_identifier_named(
        &mut self,
        expected_name: &str,
    ) -> Result<(), ParseError> {
        let span = self.current_span();
        let identifier = self.expect_identifier()?;
        if identifier.name.as_str() != expected_name {
            return Err(ParseError::UnexpectedToken {
                span: span_to_source_span(&span),
                expected: expected_name.into(),
                found: identifier.name.to_string().into(),
            });
        }
        Ok(())
    }

    /// Expect and consume an identifier token, returning just the string (for backward compatibility)
    pub(crate) fn expect_identifier_string(&mut self) -> Result<String, ParseError> {
        let identifier = self.expect_identifier()?;
        Ok(identifier.name.to_string())
    }

    /// Expect and consume a potentially namespaced identifier (e.g., "Metals.Copper")
    /// This supports namespace alias syntax: import * from @std/materials/conductors as Metals
    pub(crate) fn expect_namespaced_identifier_string(&mut self) -> Result<String, ParseError> {
        let mut name = self.expect_identifier_string()?;

        // Check if followed by a dot (namespace separator)
        if self.check(&Token::Dot) {
            self.advance(); // consume dot
            let second_part = self.expect_identifier_string()?;
            name.push('.');
            name.push_str(&second_part);
        }

        Ok(name)
    }

    /// Expect and consume a potentially namespaced identifier as an Identifier AST node
    /// This is the Identifier version of expect_namespaced_identifier_string()
    pub(crate) fn expect_namespaced_identifier(&mut self) -> Result<Identifier, ParseError> {
        let start_pos = self.current_span().start;
        let name = self.expect_namespaced_identifier_string()?;
        let end_pos = self.previous_span().end;

        Ok(Identifier {
            name: name.into(),
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Expect and consume an identifier or keyword token (for import paths)
    ///
    /// In import paths, keywords like `logic`, `test`, etc. should be treated as identifiers.
    /// This allows paths like `logic/adders` or `test/fixtures`.
    pub(crate) fn expect_identifier_or_keyword_string(&mut self) -> Result<String, ParseError> {
        self.expect_identifier_string()
    }

    /// Expect and consume an identifier or keyword token as an Identifier AST node
    pub(crate) fn expect_identifier_or_keyword(&mut self) -> Result<Identifier, ParseError> {
        self.expect_identifier()
    }

    /// Expect and consume a string token
    pub(crate) fn expect_string(&mut self) -> Result<String, ParseError> {
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

    /// Expect and consume an integer token
    pub(crate) fn expect_integer(&mut self) -> Result<usize, ParseError> {
        if let Some(current) = self.current() {
            if let Token::Integer(n) = &current.token {
                if *n < 0 {
                    return Err(ParseError::General {
                        span: span_to_source_span(&current.span),
                        message: "Expected positive integer".into(),
                    });
                }
                let result = *n as usize;
                self.advance();
                Ok(result)
            } else {
                Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "integer".into(),
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

    /// Expect and consume a number token (integer or float)
    pub(crate) fn expect_number(&mut self) -> Result<f64, ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Integer(n) => {
                    let result = *n as f64;
                    self.advance();
                    Ok(result)
                }
                Token::Float(n) => {
                    let result = *n;
                    self.advance();
                    Ok(result)
                }
                _ => Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "number".into(),
                    found: format!("{}", current.token).into(),
                }),
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

    /// Expect and consume a boolean token (true or false)
    pub(crate) fn expect_boolean(&mut self) -> Result<bool, ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::True => {
                    self.advance();
                    Ok(true)
                }
                Token::False => {
                    self.advance();
                    Ok(false)
                }
                _ => Err(ParseError::UnexpectedToken {
                    span: span_to_source_span(&current.span),
                    expected: "boolean (true or false)".into(),
                    found: format!("{}", current.token).into(),
                }),
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

    /// Parse a boolean value and skip trailing whitespace
    pub(crate) fn parse_boolean(&mut self) -> Result<bool, ParseError> {
        let val = self.expect_boolean()?;
        self.skip_whitespace();
        Ok(val)
    }
}
