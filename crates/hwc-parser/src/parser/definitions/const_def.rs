//! Parser for constant definitions (v0.1.6)

use crate::ast::ConstDefinition;
use crate::lexer::Token;
use crate::parser::Parser;

impl Parser {
    /// Parse constant definition: `const NAME: value`
    ///
    /// Syntax:
    /// ```hw
    /// const PI: 3.14159265358979323846
    /// const SPEED_OF_LIGHT: 299792458
    /// ```
    pub(super) fn parse_const(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<ConstDefinition> {
        let start = self.current_span().start;

        // Expect 'const' keyword
        if let Err(e) = self.expect(&Token::Const) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // Expect constant name (identifier)
        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // Expect numeric value (float or integer)
        let value = match self.current().map(|t| &t.token) {
            Some(Token::Float(f)) => {
                let val = *f;
                self.advance();
                val
            }
            Some(Token::Integer(i)) => {
                let val = *i as f64;
                self.advance();
                val
            }
            _ => {
                let err = self.error("Expected numeric value after constant name");
                collector.report(err);
                self.sync_to_next_definition();
                return None;
            }
        };

        // Skip optional newlines
        self.skip_newlines();

        let end = self.previous_span().end;

        Some(ConstDefinition {
            name: name.name,
            value,
            span: crate::lexer::Span::new(start, end),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::DiagnosticCollector;

    #[test]
    fn test_parse_const_float() {
        let input = "const PI: 3.14159265358979323846";
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(input, 100);

        let result = parser.parse_const(&collector);
        assert!(result.is_some(), "Should parse const with float value");

        let const_def = result.unwrap();
        assert_eq!(const_def.name, "PI");
        assert!((const_def.value - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn test_parse_const_integer() {
        let input = "const SPEED_OF_LIGHT: 299792458";
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(input, 100);

        let result = parser.parse_const(&collector);
        assert!(result.is_some(), "Should parse const with integer value");

        let const_def = result.unwrap();
        assert_eq!(const_def.name, "SPEED_OF_LIGHT");
        assert_eq!(const_def.value, 299792458.0);
    }

    #[test]
    fn test_parse_const_scientific() {
        let input = "const VACUUM_PERMITTIVITY: 8.854187817e-12";
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(input, 100);

        let result = parser.parse_const(&collector);
        assert!(
            result.is_some(),
            "Should parse const with scientific notation"
        );

        let const_def = result.unwrap();
        assert_eq!(const_def.name, "VACUUM_PERMITTIVITY");
        assert!((const_def.value - 8.854187817e-12).abs() < 1e-20);
    }
}
