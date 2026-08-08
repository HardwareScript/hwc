//! Parser for constant definitions (v0.1.6)

use crate::ast::ConstDefinition;
use crate::lexer::Token;
use crate::parser::Parser;

impl<'ast> Parser<'ast> {
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
        is_exported: bool,
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
        self.skip_whitespace();

        let end = self.previous_span().end;

        Some(ConstDefinition {
            name: name.name,
            is_exported,
            value,
            span: crate::lexer::Span::new(start, end),
        })
    }
}
