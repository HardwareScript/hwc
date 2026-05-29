//! Pattern and strategy definition parsing

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Pattern Definition Parsing
    // ========================================================================

    /// Parse pattern definition: `define pattern "Zigzag" (gap: Measurement):`
    pub(in super::super) fn parse_pattern(&mut self) -> Result<PatternDefinition, ParseError> {
        let start_pos = self.current_span().start;
        // Note: 'pattern' identifier already consumed by parse_definition
        let name = self.expect_identifier()?;

        // Parse parameter list
        let params = self.parse_pattern_parameters()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        // v0.1.6: Expect 'steps' identifier
        let steps_ident = self.expect_identifier()?;
        if steps_ident.as_str() != "steps" {
            return Err(self.error(&format!("Expected 'steps', found '{}'", steps_ident)));
        }
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        // Parse steps
        let mut steps = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect dash for list item
            self.expect(&Token::Hyphen)?;

            let step_start = self.current_span().start;

            // Parse distance expression
            let distance = self.parse_expression()?;

            // Expect 'r' (rotate operator) as identifier
            self.expect_identifier_value("r")?;

            // Parse angle expression
            let angle = self.parse_expression()?;

            let step_end = self.previous_span().end;

            steps.push(PatternStep {
                distance,
                angle,
                span: Span::new(step_start, step_end),
            });

            self.skip_newlines();
        }

        if self.check(&Token::Dedent) {
            self.advance(); // steps dedent
        }

        if self.check(&Token::Dedent) {
            self.advance(); // pattern dedent
        }

        let end_pos = self.previous_span().end;

        Ok(PatternDefinition {
            name,
            params,
            steps,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse pattern parameters: `(gap: Measurement, amp: Measurement)`
    fn parse_pattern_parameters(&mut self) -> Result<Vec<PatternParameter>, ParseError> {
        self.expect(&Token::OpenParen)?;

        let mut params = Vec::new();

        // Handle empty parameter list
        if self.check(&Token::CloseParen) {
            self.advance();
            return Ok(params);
        }

        loop {
            let param_start = self.current_span().start;
            let name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;
            let type_name = self.expect_namespaced_identifier()?;

            let param_type = match type_name.as_str() {
                "Measurement" => ParameterType::Measurement,
                "Number" => ParameterType::Number,
                "String" => ParameterType::String,
                _ => {
                    return Err(self.error(&format!(
                        "Unknown parameter type '{}'. Expected: Measurement, Number, or String",
                        type_name
                    )))
                }
            };

            let param_end = self.previous_span().end;

            params.push(PatternParameter {
                name: name.into(),
                param_type,
                span: Span::new(param_start, param_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::CloseParen)?;

        Ok(params)
    }

    // ========================================================================
    // Strategy Definition Parsing
    // ========================================================================

    /// Parse strategy definition: `define strategy "DDR5_Match":`
    pub(in super::super) fn parse_strategy(&mut self) -> Result<StrategyDefinition, ParseError> {
        let start_pos = self.current_span().start;
        // Note: 'strategy' identifier already consumed by parse_definition
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut target = None;
        let mut tolerance = None;
        let mut pattern = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Check for strategy block identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "target" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            target = Some(self.parse_strategy_target()?);
                            self.skip_newlines();
                            continue;
                        }
                        "tolerance" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            tolerance = Some(self.parse_measurement()?);
                            self.skip_newlines();
                            continue;
                        }
                        "pattern" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            pattern = Some(self.parse_pattern_instantiation()?);
                            self.skip_newlines();
                            continue;
                        }
                        _ => {
                            let field_name = name.clone();
                            return Err(self.error(&format!(
                                "Unknown strategy field: '{}'. Expected: target, tolerance, or pattern",
                                field_name
                            )));
                        }
                    }
                }
            }

            // If we get here, it's not an identifier - break
            break;
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(StrategyDefinition {
            name,
            target,
            tolerance,
            pattern,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse strategy target: match_longest, match_shortest, or measurement
    fn parse_strategy_target(&mut self) -> Result<StrategyTarget, ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Identifier(ident) => {
                    let target = match ident.as_str() {
                        "match_longest" => StrategyTarget::MatchLongest,
                        "match_shortest" => StrategyTarget::MatchShortest,
                        _ => {
                            return Err(self.error(&format!(
                                "Unknown strategy target '{}'. Expected: match_longest, match_shortest, or a measurement",
                                ident
                            )))
                        }
                    };
                    self.advance();
                    Ok(target)
                }
                Token::Measurement(_) => {
                    let measurement = self.parse_measurement()?;
                    Ok(StrategyTarget::Specific(measurement))
                }
                _ => Err(self.error(
                    "Expected strategy target (match_longest, match_shortest, or measurement)",
                )),
            }
        } else {
            Err(self.error("Expected strategy target"))
        }
    }

    /// Parse pattern instantiation: `Trombone(gap: 0.3mm, amp: 2.5mm)`
    pub(in super::super) fn parse_pattern_instantiation(
        &mut self,
    ) -> Result<PatternInstantiation, ParseError> {
        let start_pos = self.current_span().start;
        let name = self.expect_identifier_string()?;

        self.expect(&Token::OpenParen)?;

        let mut arguments = Vec::new();

        // Handle empty argument list
        if self.check(&Token::CloseParen) {
            self.advance();
            let end_pos = self.previous_span().end;
            return Ok(PatternInstantiation {
                name: name.into(),
                arguments,
                span: Span::new(start_pos, end_pos),
            });
        }

        loop {
            let arg_start = self.current_span().start;
            let arg_name = self.expect_identifier_string()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_expression()?;
            let arg_end = self.previous_span().end;

            arguments.push(PatternArgument {
                name: arg_name.into(),
                value,
                span: Span::new(arg_start, arg_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::CloseParen)?;

        let end_pos = self.previous_span().end;

        Ok(PatternInstantiation {
            name: name.into(),
            arguments,
            span: Span::new(start_pos, end_pos),
        })
    }
}
