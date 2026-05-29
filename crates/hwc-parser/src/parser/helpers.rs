//! Parser helper methods for token manipulation and common operations

use super::error::{span_to_source_span, ParseError};
use crate::ast::Identifier;
use crate::lexer::{Span, SpannedToken, Token};
use compact_str::CompactString;
use miette::SourceSpan;

impl super::Parser {
    // ========================================================================
    // Token Navigation
    // ========================================================================

    /// Get the current token without consuming it
    ///
    /// NOTE: This automatically skips comment tokens, making them invisible to all parsing logic.
    /// Comments are handled at the lexer/parser boundary, not in individual parsing functions.
    pub(super) fn current(&self) -> Option<&SpannedToken> {
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
    #[allow(dead_code)]
    pub(super) fn peek_ahead(&self, offset: usize) -> Option<&SpannedToken> {
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
    pub(super) fn advance(&mut self) {
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

    /// Check if current token matches the given token type
    pub(super) fn check(&self, token: &Token) -> bool {
        if let Some(current) = self.current() {
            &current.token == token
        } else {
            false
        }
    }

    /// Consume the current token if it matches, otherwise return error
    pub(super) fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
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
    pub(super) fn is_at_end(&self) -> bool {
        if let Some(current) = self.current() {
            matches!(current.token, Token::Eof)
        } else {
            true
        }
    }

    // ========================================================================
    // Error Handling
    // ========================================================================

    /// Create an error at the current position
    pub(super) fn error(&self, message: &str) -> ParseError {
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

    // ========================================================================
    // Statement Terminators (for whitespace-significant languages)
    // ========================================================================

    /// Consume a statement terminator (newline, dedent, or EOF)
    ///
    /// In whitespace-significant languages like Python and Hardware Script,
    /// a statement can end in three ways:
    /// 1. Explicit newline (most common)
    /// 2. Dedentation (end of block)
    /// 3. End of file
    /// 4. Next statement starts (after a block expression like match)
    ///
    /// This method handles all cases gracefully. If a newline is present,
    /// it consumes it to keep the stream clean. Otherwise, it just returns Ok()
    /// and lets the main loop continue parsing.
    pub(super) fn consume_statement_end(&mut self) -> Result<(), ParseError> {
        // If there is a newline, cleanly consume it
        if self.check(&Token::Newline) {
            self.advance();
            return Ok(());
        }

        // If we are at a dedent or EOF, that's fine too
        // (The dedent will be consumed by the parent block parser)
        if self.check(&Token::Dedent) || self.check(&Token::Eof) {
            return Ok(());
        }

        // If a statement ended with a block (like 'match'), the next token
        // will just be the start of the next statement (e.g., an Identifier).
        // This is valid! Just return Ok() and let the main loop continue.
        // If it's actually garbage (e.g., `a = 1 garbage`), the outer loop
        // will naturally fail with "Unexpected identifier 'garbage'".
        Ok(())
    }

    // ========================================================================
    // Whitespace and Comments
    // ========================================================================

    /// Skip newline tokens only
    ///
    /// NOTE: Comments are now automatically skipped by advance() and current(),
    /// so this method only needs to handle newlines.
    pub(super) fn skip_whitespace(&mut self) {
        while let Some(current) = self.current() {
            match current.token {
                Token::Newline => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    /// Skip newline tokens (deprecated - use skip_whitespace instead)
    pub(super) fn skip_newlines(&mut self) {
        self.skip_whitespace();
    }

    /// Collect any doc comments/blocks before the current position
    ///
    /// NOTE: This method needs direct token access to collect comments
    /// before they're skipped by the automatic comment filtering.
    pub(super) fn collect_doc_comments(&mut self) -> Vec<CompactString> {
        let mut docs = Vec::new();

        // Directly access tokens to collect doc comments
        while self.current < self.tokens.len() {
            match &self.tokens[self.current].token {
                Token::DocComment(content) | Token::DocBlock(content) => {
                    docs.push(content.clone().into());
                    self.current += 1; // Use raw increment to bypass comment skipping
                }
                Token::Newline | Token::BlockComment(_) => {
                    self.current += 1; // Use raw increment
                }
                _ => break,
            }
        }

        docs
    }

    // ========================================================================
    // Token Extraction
    // ========================================================================

    /// Expect and consume an identifier token, returning Identifier with span
    pub(super) fn expect_identifier(&mut self) -> Result<crate::ast::Identifier, ParseError> {
        if let Some(current) = self.current() {
            // v0.1.6 Migration: Detect quoted string (old v0.1.5 syntax)
            if let Token::String(_) = &current.token {
                return Err(crate::parser::error::error_expected_identifier_not_string(
                    &current.span,
                ));
            }

            if let Token::Identifier(name) = &current.token {
                let identifier = crate::ast::Identifier::new(name.clone().into(), current.span);
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
    pub(super) fn expect_identifier_named(&mut self, expected_name: &str) -> Result<(), ParseError> {
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
    pub(super) fn expect_identifier_string(&mut self) -> Result<String, ParseError> {
        let identifier = self.expect_identifier()?;
        Ok(identifier.name.to_string())
    }

    /// Expect and consume a potentially namespaced identifier (e.g., "Metals.Copper")
    /// This supports namespace alias syntax: import * from @std/materials/conductors as Metals
    pub(super) fn expect_namespaced_identifier_string(&mut self) -> Result<String, ParseError> {
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
    pub(super) fn expect_namespaced_identifier(&mut self) -> Result<Identifier, ParseError> {
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
    pub(super) fn expect_identifier_or_keyword_string(&mut self) -> Result<String, ParseError> {
        if let Some(current) = self.current() {
            let name = match &current.token {
                Token::Identifier(name) => name.clone(),
                // Allow keywords as identifiers in import paths
                Token::Logic => "logic".into(),
                Token::Test => "test".into(),
                Token::Component => "component".into(),
                Token::Space => "space".into(),
                Token::Material => "material".into(),
                Token::Profile => "profile".into(),
                Token::Module => "module".into(),
                Token::Enum => "enum".into(),
                Token::Struct => "struct".into(),
                Token::Unit => "unit".into(),
                Token::Device => "device".into(),
                Token::SignalGroup => "signal_group".into(),
                Token::Mechanical => "mechanical".into(),
                Token::Interface => "interface".into(),
                Token::Bridge => "bridge".into(),
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        span: span_to_source_span(&current.span),
                        expected: "identifier or keyword".into(),
                        found: format!("{}", current.token).into(),
                    })
                }
            };
            self.advance();
            Ok(name)
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            Err(ParseError::UnexpectedEof { span })
        }
    }

    /// Expect and consume an identifier or keyword token as an Identifier AST node
    pub(super) fn expect_identifier_or_keyword(&mut self) -> Result<Identifier, ParseError> {
        if let Some(current) = self.current() {
            let name = match &current.token {
                Token::Identifier(name) => name.clone(),
                Token::Logic => "logic".into(),
                Token::Test => "test".into(),
                Token::Component => "component".into(),
                Token::Space => "space".into(),
                Token::Material => "material".into(),
                Token::Profile => "profile".into(),
                Token::Module => "module".into(),
                Token::Enum => "enum".into(),
                Token::Struct => "struct".into(),
                Token::Unit => "unit".into(),
                Token::Device => "device".into(),
                Token::SignalGroup => "signal_group".into(),
                Token::Mechanical => "mechanical".into(),
                Token::Interface => "interface".into(),
                Token::Bridge => "bridge".into(),
                _ => {
                    return Err(ParseError::UnexpectedToken {
                        span: span_to_source_span(&current.span),
                        expected: "identifier or keyword".into(),
                        found: format!("{}", current.token).into(),
                    })
                }
            };
            let identifier = Identifier::new(name.into(), current.span);
            self.advance();
            Ok(identifier)
        } else {
            let span = if let Some(last) = self.tokens.last() {
                span_to_source_span(&last.span)
            } else {
                SourceSpan::new(0.into(), 0.into())
            };
            Err(ParseError::UnexpectedEof { span })
        }
    }

    /// Parse a net name with optional array index (v0.1.6 Sprint 3.4)
    ///
    /// Supports both simple names (`VDD`) and indexed names (`Bus[i]`)
    ///
    /// Examples:
    /// - `net: VDD` → NetName::simple("VDD")
    /// - `net: Bus[i]` → NetName::indexed("Bus", Expression::Variable("i"))
    /// - `net: D[0]` → NetName::indexed("D", Expression::Literal(0))
    pub(super) fn parse_net_name(&mut self) -> Result<crate::ast::NetName, ParseError> {
        let start_pos = self.current_span().start;
        let base_name = self.expect_identifier_string()?;

        // Check for array index: [i] or [0]
        if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            let end_pos = self.previous_span().end;

            Ok(crate::ast::NetName::indexed(
                base_name.into(),
                index_expr,
                Span::new(start_pos, end_pos),
            ))
        } else {
            let end_pos = self.previous_span().end;
            Ok(crate::ast::NetName::simple(
                base_name.into(),
                Span::new(start_pos, end_pos),
            ))
        }
    }

    /// Expect and consume a string token
    pub(super) fn expect_string(&mut self) -> Result<String, ParseError> {
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
    pub(super) fn expect_integer(&mut self) -> Result<usize, ParseError> {
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

    // ========================================================================
    // Span Utilities
    // ========================================================================

    /// Get the span of the current token
    pub(super) fn current_span(&self) -> Span {
        if let Some(token) = self.current() {
            token.span
        } else {
            // Return a zero-width span at the end
            Span::new(0, 0)
        }
    }

    /// Get the span of the previous token
    pub(super) fn previous_span(&self) -> Span {
        if self.current > 0 {
            self.tokens[self.current - 1].span
        } else {
            Span::new(0, 0)
        }
    }

    /// Expect and consume a number token (integer or float)
    pub(super) fn expect_number(&mut self) -> Result<f64, ParseError> {
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
    pub(super) fn expect_boolean(&mut self) -> Result<bool, ParseError> {
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
    pub(super) fn parse_boolean(&mut self) -> Result<bool, ParseError> {
        let val = self.expect_boolean()?;
        self.skip_whitespace();
        Ok(val)
    }

    pub(super) fn skip_until_newline(&mut self) {
        while !self.is_at_end() && !self.check(&Token::Newline) && !self.check(&Token::Dedent) {
            self.advance();
        }
    }

    /// Parse pin reference: Component.Pin, Component[i].Pin, Component.Pin[i+1], or Component[i].Pin[j]
    ///
    /// Supports parametric indices with loop variables and expressions (Sprint 3.10):
    /// - `Adder[0].carry_out` - literal index
    /// - `Adder[i].carry_out` - loop variable
    /// - `Adder[i+1].carry_in` - expression with loop variable
    pub(super) fn parse_pin_reference(&mut self) -> Result<crate::ast::PinReference, ParseError> {
        let start_pos = self.current_span().start;
        let first = self.expect_identifier_string()?;

        // Check for array index on component: Component[i] or Component[i+1]
        let component_index = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index_expr = self.parse_expression()?;
            self.expect(&Token::CloseBracket)?;
            Some(index_expr)
        } else {
            None
        };

        // Check if there's a dot (component.pin syntax)
        if self.check(&Token::Dot) {
            self.advance();
            let pin = self.expect_identifier_string()?;

            // Check for array index on pin: Pin[i] or Pin[i-1]
            let pin_index = if self.check(&Token::OpenBracket) {
                self.advance(); // consume '['
                let index_expr = self.parse_expression()?;
                self.expect(&Token::CloseBracket)?;
                Some(index_expr)
            } else {
                None
            };

            let end_pos = self.previous_span().end;

            Ok(crate::ast::PinReference {
                component: first.into(),
                component_index,
                pin: pin.into(),
                pin_index,
                span: Span::new(start_pos, end_pos),
            })
        } else {
            // Just a pin name (component is implicit or will be resolved later)
            let end_pos = self.previous_span().end;
            Ok(crate::ast::PinReference {
                component: String::new().into(), // Empty component means implicit/to be resolved
                component_index: None,
                pin: first.into(),
                pin_index: component_index, // If we parsed [expr], it's actually the pin index
                span: Span::new(start_pos, end_pos),
            })
        }
    }

    /// Get a human-readable description of a token for error messages
    pub(super) fn token_description(&self, token: &Token) -> CompactString {
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

    // ========================================================================
    // List Parser (canonical bracket syntax only)
    // ========================================================================

    /// Parse a list using canonical bracket syntax only.
    ///
    /// ONLY supported format (post pre-release cleanup):
    ///   Bracket notation: `[A, B, C]` or `[A, B, C,]` (trailing comma allowed)
    ///
    /// Legacy inline (bare `A, B, C`) and block (indented newline lists) formats
    /// REMOVED to eliminate backward-compat complexity in the parser.
    ///
    /// # Rationale for removal (see also space.rs deprecated fields cleanup):
    /// Pre-1.0: Supporting 3 syntaxes for lists bloated the parser (parse_list + 2 helpers + special error paths),
    /// led to ambiguous parsing edge cases (e.g. with newlines in other contexts), and made error messages worse.
    /// Mistake pattern avoided: "We'll support the old way just during transition" — this debt accumulated.
    /// Always enforce canonical syntax from the start in 0.x; update all tests/examples at once.
    ///
    /// # Arguments
    /// * `item_parser` - Function to parse a single list item
    ///
    /// # Returns
    /// * `Ok(Vec<T>)` - Parsed list items
    /// * `Err(ParseError)` - If not starting with `[` or other parse failure
    ///
    /// # Examples
    /// ```ignore
    /// // Parse pin list: [VCC, GND, SDA]
    /// let pins = self.parse_list(|p| p.expect_identifier_string())?;
    /// ```
    pub(super) fn parse_list<T, F>(&mut self, item_parser: F) -> Result<Vec<T>, ParseError>
    where
        F: FnMut(&mut Self) -> Result<T, ParseError>,
    {
        // Only bracket notation is supported (legacy formats removed)
        if self.check(&Token::OpenBracket) {
            return self.parse_bracket_list(item_parser);
        }

        // Legacy formats rejected with clear error
        Err(self.error("Lists must use bracket notation, e.g. [A, B, C]. Legacy inline 'A, B' and indented block list formats were removed in pre-release cleanup."))
    }

    /// Parse bracket notation list: `[A, B, C]` or `[A, B, C,]`
    fn parse_bracket_list<T, F>(&mut self, mut item_parser: F) -> Result<Vec<T>, ParseError>
    where
        F: FnMut(&mut Self) -> Result<T, ParseError>,
    {
        self.expect(&Token::OpenBracket)?;
        let mut items = Vec::new();

        // Skip whitespace after opening bracket
        self.skip_whitespace();

        // Handle empty list: []
        if self.check(&Token::CloseBracket) {
            self.advance();
            return Ok(items);
        }

        // Parse first item
        items.push(item_parser(self)?);
        self.skip_whitespace();

        // Parse remaining items separated by commas
        while self.check(&Token::Comma) {
            self.advance(); // consume comma
            self.skip_whitespace();

            // Check for trailing comma: [A, B, C,]
            if self.check(&Token::CloseBracket) {
                break;
            }

            items.push(item_parser(self)?);
            self.skip_whitespace();
        }

        self.expect(&Token::CloseBracket)?;
        Ok(items)
    }

    /// Parse a declarative property block (uses `:` for key-value pairs).
    ///
    /// This is the universal property parser for v0.1.6's "Boundary Law":
    /// - Declarative contexts (properties) use `:` (colon)
    /// - Behavioral contexts (logic) use `=` (equals)
    ///
    /// This parser is used for:
    /// - Component property blocks: `electrical:`, `mechanical:`, `thermal:`
    /// - Material properties
    /// - Profile constraints
    /// - Any other declarative key-value configuration
    ///
    /// # Format
    /// ```ignore
    /// electrical:
    ///     resistance: 10kΩ
    ///     tolerance: 5%
    ///     power: 0.125W
    /// ```
    ///
    /// # Returns
    /// * `Ok(HashMap<String, String>)` - Successfully parsed properties
    /// * `Err(ParseError)` - Failed to parse (with context-aware error message.into())
    ///
    /// # Error Messages
    /// If `=` is found instead of `:`, the error message teaches the boundary rule:
    /// "Expected ':' in property block (use '=' only in logic blocks)"
    pub(super) fn parse_property_block(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, String>, ParseError> {
        let mut properties = rustc_hash::FxHashMap::default();

        // Expect newline after the property block name
        self.expect(&Token::Newline)?;

        // Skip any newlines before the indent (comments are auto-skipped)
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Now expect the indent
        self.expect(&Token::Indent)?;

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            // Skip blank lines (comments are auto-skipped)
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // Check for dedent (end of block)
            if self.check(&Token::Dedent) {
                break;
            }

            // Check if we've hit the start of another block
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    if matches!(
                        name.as_str(),
                        "render"
                            | "layout"
                            | "pins"
                            | "metadata"
                            | "electrical"
                            | "mechanical"
                            | "thermal"
                    ) {
                        break;
                    }
                }
            }

            // Parse property key (must be an identifier or keyword used as identifier)
            // This allows "soft keywords" like 'material', 'profile', 'category' as property names
            let key = self.expect_identifier_or_keyword_string()?;

            // THE BOUNDARY LAW: Expect colon in declarative contexts
            if self.check(&Token::Equals) {
                return Err(crate::parser::error::error_expected_colon_in_property(
                    &self.current_span(),
                ));
            }

            self.expect(&Token::Colon)?;

            // Check for optional negative sign
            let is_negative = if self.check(&Token::Hyphen) {
                self.advance();
                true
            } else {
                false
            };

            // Parse property value
            let value = if let Some(spanned) = self.current() {
                match &spanned.token {
                    Token::Measurement(m) => {
                        let sign = if is_negative { "-" } else { "" };
                        let val = format!("{}{}{}", sign, m.value, m.unit);
                        self.advance();
                        val
                    }
                    Token::Integer(n) => {
                        let sign = if is_negative { "-" } else { "" };
                        let val = format!("{}{}", sign, n);
                        self.advance();
                        val
                    }
                    Token::Float(f) => {
                        let sign = if is_negative { "-" } else { "" };
                        let val = format!("{}{}", sign, f);
                        self.advance();
                        val
                    }
                    Token::String(s) => {
                        let val = s.clone();
                        self.advance();
                        val
                    }
                    Token::Identifier(id) => {
                        let val = id.clone();
                        self.advance();
                        val
                    }
                    Token::True => {
                        self.advance();
                        "true".into()
                    }
                    Token::False => {
                        self.advance();
                        "false".into()
                    }
                    _ => {
                        return Err(self.error(&format!(
                            "Expected property value (measurement, number, string, or identifier), found {:?}",
                            spanned.token
                        )));
                    }
                }
            } else {
                return Err(self.error("Expected property value"));
            };

            properties.insert(key, value);

            // Skip any inline comments and whitespace after the value
            self.skip_whitespace();
        }

        // Consume the dedent that ends the property block
        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(properties
            .into_iter()
            .map(|(k, v): (String, String)| (k.into(), v))
            .collect())
    }
}
