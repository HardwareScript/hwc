use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::Parser;

impl Parser {
    pub fn parse_logic_block(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<LogicBlock> {
        let start = self.current_span();

        if let Err(e) = self.expect(&Token::Logic) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let mut statements = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            match self.parse_logic_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_logic_statement();

                    if collector.should_stop() {
                        break;
                    }
                }
            }
        }

        if let Err(e) = self.expect(&Token::Dedent) {
            collector.report(e);
        }

        let span = Span::new(start.start, self.previous_span().end);

        Some(LogicBlock { statements, span })
    }

    fn sync_to_next_logic_statement(&mut self) {
        while let Some(token) = self.current() {
            match &token.token {
                Token::Let | Token::If | Token::Match => break,
                Token::Dedent | Token::Newline => {
                    self.advance();
                    break;
                }
                _ => self.advance(),
            }
        }
    }
}
