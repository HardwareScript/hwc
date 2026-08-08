use crate::ast::{PinDeclaration, Span};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl<'ast> Parser<'ast> {
    /// Parse module pins block
    ///
    /// Supports both inline and block syntax:
    /// ```hw
    /// # Inline:
    /// pins: VCC, GND, Bus_A[64]
    ///
    /// # Block:
    /// pins:
    ///     VCC
    ///     GND
    ///     Bus_A[64]
    /// ```
    pub(super) fn parse_module_pins(&mut self) -> Result<Vec<PinDeclaration>, ParseError> {
        // v0.1.6: 'pins' is now an identifier
        self.expect_identifier()?; // consume 'pins'
        self.expect(&Token::Colon)?;

        // Use universal list parser (v0.1.6)
        self.parse_list(|parser| parser.parse_module_pin_declaration())
    }

    /// Parse property-style pin role declaration (v0.1.6 Context-Aware Parsing)
    ///
    /// Syntax:
    /// ```hw
    /// input: VIN
    /// output: VOUT, VOUT2
    /// power: VDD
    /// ground: GND
    /// inout: DATA[8]
    /// ```
    pub(super) fn parse_pin_role_property(&mut self) -> Result<Vec<PinDeclaration>, ParseError> {
        use crate::PinDirection;

        // Parse the direction identifier (input, output, power, ground, inout)
        let direction_name = self.expect_identifier_string()?;
        let direction = match direction_name.as_str() {
            "input" => PinDirection::Input,
            "output" => PinDirection::Output,
            "power" => PinDirection::Power,
            "ground" => PinDirection::Ground,
            "inout" => PinDirection::Inout,
            _ => {
                return Err(self.error(&format!(
                    "Expected pin direction (input, output, power, ground, inout), found '{}'",
                    direction_name
                )))
            }
        };

        self.expect(&Token::Colon)?;

        // Parse pin list (supports inline comma-separated or block format)
        let pin_names = self.parse_list(|parser| {
            let start = parser.current_span();
            let name = parser.expect_identifier_string()?;

            // Check for array syntax: Bus[64]
            let array_size = if parser.check(&Token::OpenBracket) {
                parser.advance(); // consume '['
                let size = parser.expect_integer()?;
                parser.expect(&Token::CloseBracket)?;
                Some(size)
            } else {
                None
            };

            let span = Span::new(start.start, parser.previous_span().end);

            Ok(PinDeclaration {
                name: name.into(),
                direction,
                array_size,
                span,
            })
        })?;

        Ok(pin_names)
    }

    /// Parse a single module pin declaration: Name or Name[size]
    ///
    /// Supports optional direction keywords (context-aware):
    /// - `input VIN` - input pin (legacy bracket style)
    /// - `output VOUT` - output pin (legacy bracket style)
    /// - `power VDD` - power pin (legacy bracket style)
    /// - `ground GND` - ground pin (legacy bracket style)
    /// - `inout DATA` - bidirectional pin (legacy bracket style)
    /// - `VCC` - directionless pin (defaults to Passive)
    pub(super) fn parse_module_pin_declaration(&mut self) -> Result<PinDeclaration, ParseError> {
        let start = self.current_span();

        // Step 1: Check for optional direction keyword (context-aware soft keywords)
        use crate::PinDirection;
        let direction = if let Some(current) = self.current() {
            if let Token::Identifier(name) = &current.token {
                match name.as_str() {
                    "input" => {
                        self.advance();
                        PinDirection::Input
                    }
                    "output" => {
                        self.advance();
                        PinDirection::Output
                    }
                    "power" => {
                        self.advance();
                        PinDirection::Power
                    }
                    "ground" => {
                        self.advance();
                        PinDirection::Ground
                    }
                    "inout" => {
                        self.advance();
                        PinDirection::Inout
                    }
                    _ => PinDirection::Passive, // Default - no direction specified
                }
            } else {
                PinDirection::Passive
            }
        } else {
            PinDirection::Passive
        };

        // Step 2: Parse the pin name (bare identifier)
        let name = self.expect_identifier_string()?;

        // Step 3: Check for array syntax: Bus[64]
        let array_size = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let size = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Some(size)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(PinDeclaration {
            name: name.into(),
            direction,
            array_size,
            span,
        })
    }
}
