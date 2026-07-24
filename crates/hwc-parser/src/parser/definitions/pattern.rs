//! Pattern and strategy definition parsing

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
    // ========================================================================
    // Pattern Definition Parsing
    // ========================================================================

    /// Parse pattern definition (v0.1.8 syntax):
    /// ```hardware
    /// pattern Zigzag (gap: Measurement):
    ///     strategy_goal = delay_line
    ///     steps: [
    ///         [length = gap, angle = 45],
    ///         [length = gap, angle = -45],
    ///     ]
    /// ```
    pub(in super::super) fn parse_pattern(&mut self) -> Result<PatternDefinition, ParseError> {
        let start_pos = self.current_span().start;
        let name = self.expect_identifier()?;

        let params = self.parse_pattern_parameters()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut strategy_goal = None;
        let mut steps = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if let Some(current) = self.current() {
                if let Token::Identifier(ident) = &current.token {
                    match ident.as_str() {
                        "strategy_goal" => {
                            self.advance();
                            self.expect(&Token::Equals)?;
                            let goal = self.expect_identifier_string()?;
                            strategy_goal = Some(goal.into());
                            self.skip_whitespace();
                            continue;
                        }
                        "steps" => {
                            self.advance();
                            self.expect(&Token::Colon)?;
                            self.expect(&Token::OpenBracket)?;
                            steps = self.parse_pattern_steps_array()?;
                            self.skip_whitespace();
                            continue;
                        }
                        other => {
                            return Err(self.error(&format!(
                                "Unknown pattern field: '{}'. Expected: strategy_goal, steps",
                                other
                            )));
                        }
                    }
                }
            }

            break;
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(PatternDefinition {
            name,
            is_exported: false,
            params,
            strategy_goal,
            steps,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse pattern steps array: `[ [length = gap, angle = 45], ... ]`
    fn parse_pattern_steps_array(&mut self) -> Result<Vec<PatternStep>, ParseError> {
        let mut steps = Vec::new();

        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBracket) || self.is_at_end() {
                break;
            }

            let step = self.parse_pattern_step()?;
            steps.push(step);

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBracket)?;

        Ok(steps)
    }

    /// Parse a single pattern step: `[length = gap, angle = 45]`
    fn parse_pattern_step(&mut self) -> Result<PatternStep, ParseError> {
        let start_pos = self.current_span().start;
        self.expect(&Token::OpenBracket)?;

        let mut distance = None;
        let mut angle = None;

        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBracket) || self.is_at_end() {
                break;
            }

            let field_name = self.expect_identifier_string()?;
            self.expect(&Token::Equals)?;

            match field_name.as_str() {
                "length" => {
                    distance = Some(self.parse_expression()?);
                }
                "angle" => {
                    angle = Some(self.parse_expression()?);
                }
                other => {
                    return Err(self.error(&format!(
                        "Unknown step field: '{}'. Expected: length, angle",
                        other
                    )));
                }
            }

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBracket)?;

        let end_pos = self.previous_span().end;

        let distance = distance.ok_or_else(|| self.error("Pattern step missing 'length' field"))?;
        let angle = angle.ok_or_else(|| self.error("Pattern step missing 'angle' field"))?;

        Ok(PatternStep {
            distance,
            angle,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse pattern parameters: `(gap: Measurement, amp: Measurement)`
    fn parse_pattern_parameters(&mut self) -> Result<Vec<PatternParameter>, ParseError> {
        self.expect(&Token::OpenParen)?;

        let mut params = Vec::new();

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

    /// Parse strategy definition (v0.1.8 syntax):
    /// ```hardware
    /// strategy DDR5_Match:
    ///     target = match_longest
    ///     tolerance = 0.1mm
    ///     pattern = Trombone(gap: 0.3mm, amp: 2.5mm)
    /// ```
    pub(in super::super) fn parse_strategy(&mut self) -> Result<StrategyDefinition, ParseError> {
        let start_pos = self.current_span().start;
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

            if let Some(current) = self.current() {
                if let Token::Identifier(field_name) = &current.token {
                    match field_name.as_str() {
                        "target" => {
                            self.advance();
                            self.expect(&Token::Equals)?;
                            target = Some(self.parse_strategy_target()?);
                            self.skip_whitespace();
                            continue;
                        }
                        "tolerance" => {
                            self.advance();
                            self.expect(&Token::Equals)?;
                            tolerance = Some(self.parse_measurement()?);
                            self.skip_whitespace();
                            continue;
                        }
                        "pattern" => {
                            self.advance();
                            self.expect(&Token::Equals)?;
                            pattern = Some(self.parse_pattern_instantiation()?);
                            self.skip_whitespace();
                            continue;
                        }
                        other => {
                            return Err(self.error(&format!(
                                "Unknown strategy field: '{}'. Expected: target, tolerance, pattern",
                                other
                            )));
                        }
                    }
                }
            }

            break;
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(StrategyDefinition {
            name,
            is_exported: false,
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
