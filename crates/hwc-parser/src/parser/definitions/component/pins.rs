//! Pin declaration parsing

use super::super::super::error::ParseError;
use crate::lexer::Token;
use compact_str::CompactString;
use smallvec::SmallVec;

impl super::super::super::Parser {
    pub(super) fn parse_pins_block(&mut self) -> Result<SmallVec<[CompactString; 4]>, ParseError> {
        // NOTE: The caller (parse_component_def) already consumed `Token::Colon`.

        // Use universal list parser (v0.1.6)
        // Supports: [A, B, C] (bracket), A, B, C (inline), or block format
        let pins = self.parse_list(|parser| parser.parse_pin_with_optional_width())?;
        Ok(pins.into_iter().map(|s: String| s.into()).collect())
    }

    /// Parse a pin name with optional width: A or A[16]
    fn parse_pin_with_optional_width(&mut self) -> Result<String, ParseError> {
        let name = self.expect_identifier_string()?;

        // Check for optional array width
        if self.check(&Token::OpenBracket) {
            self.advance();
            let width = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Ok(format!("{}[{}]", name, width))
        } else {
            Ok(name)
        }
    }

    /// Parse a pin reference string with optional array index: A or A[0]
    /// Used in pin_positions and pad_shapes where we need string keys for HashMaps
    pub(super) fn parse_pin_reference_string(&mut self) -> Result<String, ParseError> {
        let name = self.expect_identifier_string()?;

        // Check for optional array index
        if self.check(&Token::OpenBracket) {
            self.advance();
            let index = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Ok(format!("{}[{}]", name, index))
        } else {
            Ok(name)
        }
    }
}
