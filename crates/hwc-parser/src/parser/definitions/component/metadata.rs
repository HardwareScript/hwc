//! Component metadata parsing

use super::super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::super::Parser {
    pub(super) fn parse_component_metadata(&mut self) -> Result<ComponentMetadata, ParseError> {
        let start_pos = self.current_span().start;
        let mut manufacturer = None;
        let mut part_number = None;
        let mut package = None;
        let mut value = None;
        let mut description = None;
        let mut datasheet = None;
        let mut other = rustc_hash::FxHashMap::default();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) || self.check(&Token::Indent) {
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                break;
            }

            // v0.1.6: All metadata field names are identifiers
            let key_str = match self.expect_identifier_string() {
                Ok(s) => s,
                Err(_) => {
                    // Error recovery: skip to next line
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    if self.check(&Token::Newline) {
                        self.advance();
                    }
                    continue;
                }
            };

            if self.expect(&Token::Colon).is_err() {
                // Error recovery: skip to next line
                while !self.is_at_end()
                    && !self.check(&Token::Newline)
                    && !self.check(&Token::Dedent)
                {
                    self.advance();
                }
                if self.check(&Token::Newline) {
                    self.advance();
                }
                continue;
            }

            let value_str = match self.expect_string() {
                Ok(s) => s,
                Err(_) => {
                    // Error recovery: skip to next line
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    if self.check(&Token::Newline) {
                        self.advance();
                    }
                    continue;
                }
            };

            match key_str.as_str() {
                "manufacturer" => manufacturer = Some(value_str),
                "part_number" => part_number = Some(value_str),
                "package" => package = Some(value_str),
                "value" => value = Some(value_str),
                "description" => description = Some(value_str),
                "datasheet" => datasheet = Some(value_str),
                _ => {
                    other.insert(key_str.into(), value_str);
                }
            }

            // Skip any inline comments and newlines after the value
            // This is the "Parser Cleanup" pattern - lexer strips comments but leaves newlines,
            // parser must explicitly skip them to continue parsing the next property
            self.skip_whitespace();
        }

        let end_pos = self.previous_span().end;

        Ok(ComponentMetadata {
            manufacturer: manufacturer.map(|s: String| s.into()),
            part_number: part_number.map(|s: String| s.into()),
            package: package.map(|s: String| s.into()),
            value: value.map(|s: String| s.into()),
            description: description.map(|s: String| s.into()),
            datasheet: datasheet.map(|s: String| s.into()),
            other,
            span: Span::new(start_pos, end_pos),
        })
    }
}
