use crate::lexer::{Span, Token};
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a net name with optional array index (v0.1.6 Sprint 3.4)
    ///
    /// Supports both simple names (`VDD`) and indexed names (`Bus[i]`)
    ///
    /// Examples:
    /// - `net: VDD` → NetName::simple("VDD")
    /// - `net: Bus[i]` → NetName::indexed("Bus", Expression::Variable("i"))
    /// - `net: D[0]` → NetName::indexed("D", Expression::Literal(0))
    pub(crate) fn parse_net_name(&mut self) -> Result<crate::ast::NetName, ParseError> {
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

    /// Parse route endpoint: Component.Pin, Component[i].Pin, or SpaceEntity
    ///
    /// v0.1.8: Distinguishes between ComponentPin and SpaceEntity based on presence of '.'
    pub(crate) fn parse_route_endpoint(&mut self) -> Result<crate::ast::RouteEndpointSpec, ParseError> {
        let start_pos = self.current_span().start;
        let first = self.expect_identifier_string()?;

        // Check for array index: Name[i] or Name[i+1]
        let first_index = if self.check(&Token::OpenBracket) {
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

            Ok(crate::ast::RouteEndpointSpec::ComponentPin {
                component_name: first.into(),
                component_index: first_index,
                pin_name: pin.into(),
                pin_index,
                span: Span::new(start_pos, end_pos),
            })
        } else {
            // No dot -> SpaceEntity
            let end_pos = self.previous_span().end;
            Ok(crate::ast::RouteEndpointSpec::SpaceEntity {
                name: first.into(),
                index: first_index,
                span: Span::new(start_pos, end_pos),
            })
        }
    }

    /// Parse pin reference: Component.Pin, Component[i].Pin, Component.Pin[i+1], or Component[i].Pin[j]
    ///
    /// Supports parametric indices with loop variables and expressions (Sprint 3.10):
    /// - `Adder[0].carry_out` - literal index
    /// - `Adder[i].carry_out` - loop variable
    /// - `Adder[i+1].carry_in` - expression with loop variable
    pub(crate) fn parse_pin_reference(&mut self) -> Result<crate::ast::PinReference, ParseError> {
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
    pub(crate) fn parse_list<T, F>(&mut self, item_parser: F) -> Result<Vec<T>, ParseError>
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
    pub(crate) fn parse_property_block(
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
