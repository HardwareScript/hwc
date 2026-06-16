mod blocks;
mod definitions;
mod expressions;
mod statements;

use crate::parser::ParseError;
use crate::parser::Parser;

impl Parser {
    pub(super) fn expect_identifier_value(&mut self, expected: &str) -> Result<(), ParseError> {
        if let Some(token) = self.current() {
            if let crate::lexer::Token::Identifier(name) = &token.token {
                if name == expected {
                    self.advance();
                    return Ok(());
                }
            }
        }
        Err(self.error(&format!("Expected identifier '{}'", expected)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::lexer::Lexer;
    use crate::lexer::Span;

    fn parse_logic(source: &str) -> Result<LogicBlock, ParseError> {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|e| ParseError::General {
            message: format!("Lexer error: {:?}", e).into(),
            span: crate::parser::error::span_to_source_span(&Span::new(0, 0)),
        })?;
        let mut parser = Parser::new(tokens);
        let collector = crate::DiagnosticCollector::new(source, 20);
        match parser.parse_logic_block(&collector) {
            Some(block) => {
                if collector.has_errors() {
                    Err(ParseError::General {
                        message: "Parse errors occurred".into(),
                        span: crate::parser::error::span_to_source_span(&Span::new(0, 0)),
                    })
                } else {
                    Ok(block)
                }
            }
            None => Err(ParseError::General {
                message: "Failed to parse logic block".into(),
                span: crate::parser::error::span_to_source_span(&Span::new(0, 0)),
            }),
        }
    }

    #[test]
    fn test_let_statement() {
        let source = "logic:\n    let x = 42\n";
        let result = parse_logic(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_let_mut_statement() {
        let source = "logic:\n    let mut result = 0\n";
        let result = parse_logic(source);
        assert!(result.is_ok());
    }
}
