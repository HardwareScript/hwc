//! Test definition parsing (setup, execute, assertions)

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl<'ast> super::super::Parser<'ast> {
    // ========================================================================
    // Test Definition Parsing
    // ========================================================================

    /// Parse test definition: `define test "Short Circuit Protection":`
    pub(in super::super) fn parse_test(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<TestDefinition> {
        let start_pos = self.current_span().start;

        if let Err(e) = self.expect(&Token::Test) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

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

        let mut setup = Vec::new();
        let mut execute = Vec::new();
        let mut assertions = Vec::new();

        // Parse test blocks
        let mut loop_iterations = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            loop_iterations += 1;
            let position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Test parser infinite loop detected! Breaking.");
                collector.report(
                    self.error("Test parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Check for test block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "setup" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            setup = self.parse_test_actions().unwrap_or_default();
                            continue;
                        }
                        "execute" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            execute = self.parse_test_actions().unwrap_or_default();
                            continue;
                        }
                        "assert" => {
                            self.advance();
                            if let Err(e) = self.expect(&Token::Colon) {
                                collector.report(e);
                                self.sync_to_next_definition();
                                continue;
                            }
                            assertions = self.parse_test_assertions().unwrap_or_default();
                            continue;
                        }
                        _ => {
                            let field_name = name.clone();
                            let err = self.error(&format!("Unknown test field: '{}'", field_name));
                            collector.report(err);
                            self.sync_to_next_definition();
                            continue;
                        }
                    }
                }
            }

            // Safety: Ensure we're making progress
            if self.current == position_before {
                // eprintln!("[DEBUG] Test parser didn't advance, forcing progress");
                self.advance();
            }

            // If we get here, it's not an identifier - break
            break;
        }

        // Consume the dedent that ends the test definition
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(TestDefinition {
            name,
            is_exported,
            setup,
            execute,
            assertions,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse test actions (setup or execute block)
    fn parse_test_actions(&mut self) -> Result<Vec<TestAction>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut actions = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let start_pos = self.current_span().start;
            let action_keyword = self.expect_identifier()?;

            let action_type = match action_keyword.as_str() {
                "apply" => {
                    // apply 12V to PowerSource.VIN
                    let voltage = self.parse_measurement()?;

                    self.expect(&Token::To)?;

                    let pin = self.parse_pin_reference()?;
                    TestActionType::Apply { voltage, pin }
                }
                "short" => {
                    // short Regulator.VOUT to GND
                    let from = self.parse_pin_reference()?;

                    self.expect(&Token::To)?;

                    let to = self.parse_pin_reference()?;
                    TestActionType::Short { from, to }
                }
                "wait" => {
                    // wait 1ms
                    let duration = self.parse_measurement()?;
                    TestActionType::Wait { duration }
                }
                _ => {
                    return Err(self.error(&format!("Unknown test action: '{}'", action_keyword)));
                }
            };

            self.skip_whitespace();
            let end_pos = self.previous_span().end;

            actions.push(TestAction {
                action_type,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(actions)
    }

    /// Parse test assertions
    fn parse_test_assertions(&mut self) -> Result<Vec<TestAssertion>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut assertions = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect: Regulator.VOUT < 0.5V
            let start_pos = self.current_span().start;
            let pin = self.parse_pin_reference()?;

            // Parse comparison operator (<, >, =)
            let condition = if self.check(&Token::LessThan) {
                self.advance();
                TestCondition::LessThan(self.parse_measurement()?)
            } else if self.check(&Token::GreaterThan) {
                self.advance();
                TestCondition::GreaterThan(self.parse_measurement()?)
            } else if self.check(&Token::Equals) {
                self.advance();
                // Optional double equals support by checking if there's another
                if self.check(&Token::Equals) {
                    self.advance();
                }
                TestCondition::Equals(self.parse_measurement()?)
            } else {
                return Err(self.error("Expected comparison operator (<, >, or =)"));
            };

            self.skip_whitespace();
            let end_pos = self.previous_span().end;

            assertions.push(TestAssertion {
                pin,
                condition,
                span: Span::new(start_pos, end_pos),
            });
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(assertions)
    }
}
