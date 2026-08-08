use crate::ast::Parameter; // Component instantiation parameter (enum)
use crate::lexer::Token;
use crate::parser::error::ParseError;
use smallvec::SmallVec;

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse parameters: (resistance: 10kΩ, tolerance: 1%)
    /// v0.1.6: Only keyword arguments supported for self-documenting code
    pub(in crate::parser) fn parse_parameters(
        &mut self,
    ) -> Result<SmallVec<[Parameter; 4]>, ParseError> {
        self.expect(&Token::OpenParen)?;

        let mut params = SmallVec::new();

        // Parse first parameter
        if !self.check(&Token::CloseParen) {
            params.push(self.parse_parameter()?);

            // Parse additional parameters separated by commas
            while self.check(&Token::Comma) {
                self.advance();
                params.push(self.parse_parameter()?);
            }
        }

        self.expect(&Token::CloseParen)?;

        Ok(params)
    }

    /// Parse a single parameter (keyword only - v0.1.6)
    /// Positional arguments are no longer supported for self-documenting code
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        // v0.1.6: Only keyword arguments allowed
        // Syntax: name: value
        let name = self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;
        let value = self.parse_parameter_value()?;
        Ok(Parameter::Keyword {
            name: name.into(),
            value,
        })
    }

    /// Parse a parameter value: Expression (including variables), String, or literal values
    fn parse_parameter_value(&mut self) -> Result<crate::ast::ParameterValue, ParseError> {
        if let Some(spanned) = self.current() {
            match &spanned.token {
                // String literal: "Red"
                Token::String(s) => {
                    let val = s.clone();
                    self.advance();
                    Ok(crate::ast::ParameterValue::String(val))
                }
                // Try to parse as an expression (covers measurements, numbers, variables, operations)
                _ => {
                    let expr = self.parse_expression()?;
                    Ok(crate::ast::ParameterValue::Expression(expr))
                }
            }
        } else {
            Err(self.error("Expected parameter value"))
        }
    }
}
