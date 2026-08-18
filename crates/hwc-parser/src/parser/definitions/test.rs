//! Test Definition Parser

use super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::{Span, Token};

impl super::super::Parser {
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

        let target_space = if self.check_identifier("for") {
            self.advance();
            match self.expect_identifier() {
                Ok(id) => Some(id),
                Err(e) => {
                    collector.report(e);
                    self.sync_to_next_definition();
                    return None;
                }
            }
        } else {
            None
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
        let mut directives = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let key = if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    Some(name.clone())
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(directive_name) = key {
                match directive_name.as_str() {
                    "setup" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        setup = self.parse_test_actions().unwrap_or_default();
                    }
                    "execute" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        execute = self.parse_test_actions().unwrap_or_default();
                    }
                    "assert" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        assertions = self.parse_test_assertions().unwrap_or_default();
                    }
                    "dc" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        match self.parse_dc_analysis() {
                            Ok(dc) => directives.push(SimulationDirective::Dc(dc)),
                            Err(e) => collector.report(e),
                        }
                    }
                    "ac" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        match self.parse_ac_analysis() {
                            Ok(ac) => directives.push(SimulationDirective::Ac(ac)),
                            Err(e) => collector.report(e),
                        }
                    }
                    "tran" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        match self.parse_tran_analysis() {
                            Ok(tran) => directives.push(SimulationDirective::Tran(tran)),
                            Err(e) => collector.report(e),
                        }
                    }
                    "noise" => {
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        match self.parse_noise_analysis() {
                            Ok(noise) => directives.push(SimulationDirective::Noise(noise)),
                            Err(e) => collector.report(e),
                        }
                    }
                    "op" => {
                        let span = self.current_span();
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        directives.push(SimulationDirective::Op(OpAnalysis { span }));
                    }
                    custom => {
                        let span = self.current_span();
                        let custom_id = Identifier::new(custom.into(), span);
                        self.advance();
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            continue;
                        }
                        match self.parse_generic_analysis(custom_id) {
                            Ok(gen) => directives.push(SimulationDirective::Generic(gen)),
                            Err(e) => collector.report(e),
                        }
                    }
                }
            } else {
                self.advance();
            }
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Some(TestDefinition {
            name,
            is_exported,
            target_space,
            directives,
            setup,
            execute,
            assertions,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse DC Analysis: `{ sweep: Gate, start: 0V, stop: 1.8V, step: 0.05V }`
    /// or with range syntax: `{ sweep: Gate, range: 0V..1.8V, step: 0.05V }`
    fn parse_dc_analysis(&mut self) -> Result<DcAnalysis, ParseError> {
        let span = self.current_span();
        self.expect(&Token::OpenBrace)?;

        let mut sweeps = Vec::new();
        self.parse_dc_sweep_level(&mut sweeps)?;

        Ok(DcAnalysis { sweeps, span })
    }

    fn parse_dc_sweep_level(&mut self, sweeps: &mut Vec<DcSweep>) -> Result<(), ParseError> {
        let span = self.current_span();
        let mut target = None;
        let mut start = None;
        let mut stop = None;
        let mut step = None;
        let mut scale = SweepScale::Linear;

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let key = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "sweep" => {
                    target = Some(self.parse_sweep_target()?);
                }
                "range" => {
                    let r_start = self.parse_measurement()?;
                    self.expect(&Token::Range)?;
                    let r_stop = self.parse_measurement()?;
                    start = Some(r_start);
                    stop = Some(r_stop);
                }
                "start" => start = Some(self.parse_measurement()?),
                "stop" => stop = Some(self.parse_measurement()?),
                "step" => step = Some(self.parse_measurement()?),
                "scale" => {
                    let s_id = self.expect_identifier()?;
                    scale = match s_id.as_str() {
                        "dec" => SweepScale::Decade,
                        "oct" => SweepScale::Octave,
                        "lin" => SweepScale::Linear,
                        other => {
                            return Err(self.error(&format!(
                                "Unknown sweep scale '{}'. Expected 'lin', 'dec', or 'oct'",
                                other
                            )))
                        }
                    };
                }
                "nested" => {
                    self.expect(&Token::OpenBrace)?;
                    self.parse_dc_sweep_level(sweeps)?;
                }
                unknown => {
                    return Err(self.error(&format!(
                        "Unknown DC analysis field '{}'. Valid fields: sweep, start, stop, step, range, scale, nested",
                        unknown
                    )))
                }
            }

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBrace)?;

        let target = target.ok_or_else(|| {
            self.error("DC sweep missing required field 'sweep: <NetOrParam>'")
        })?;
        let start = start.ok_or_else(|| {
            self.error("DC sweep missing required field 'start: <Value>' (or 'range: Start..Stop')")
        })?;
        let stop = stop.ok_or_else(|| {
            self.error("DC sweep missing required field 'stop: <Value>' (or 'range: Start..Stop')")
        })?;
        let step = step.ok_or_else(|| {
            self.error("DC sweep missing required field 'step: <Value>'")
        })?;

        sweeps.insert(
            0,
            DcSweep {
                target,
                start,
                stop,
                step,
                scale,
                span,
            },
        );

        Ok(())
    }

    fn parse_sweep_target(&mut self) -> Result<SweepTarget, ParseError> {
        let first_id = self.expect_identifier()?;

        if first_id.as_str().eq_ignore_ascii_case("temp") {
            return Ok(SweepTarget::Temperature);
        }

        if self.check(&Token::Dot) {
            self.advance();
            let param_id = self.expect_identifier()?;
            return Ok(SweepTarget::DeviceParam {
                device: first_id,
                param: param_id,
            });
        }

        Ok(SweepTarget::Net(first_id))
    }

    /// Parse AC Analysis: `{ sweep: dec, points: 20, freq: 100Hz..100MHz }`
    fn parse_ac_analysis(&mut self) -> Result<AcAnalysis, ParseError> {
        let span = self.current_span();
        self.expect(&Token::OpenBrace)?;

        let mut scale = SweepScale::Decade;
        let mut points = None;
        let mut start_freq = None;
        let mut stop_freq = None;

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let key = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "sweep" => {
                    let s_id = self.expect_identifier()?;
                    scale = match s_id.as_str() {
                        "dec" => SweepScale::Decade,
                        "oct" => SweepScale::Octave,
                        "lin" => SweepScale::Linear,
                        other => {
                            return Err(self.error(&format!(
                                "Unknown AC sweep scale '{}'. Expected 'dec', 'oct', or 'lin'",
                                other
                            )))
                        }
                    };
                }
                "points" => points = Some(self.expect_number()? as u32),
                "freq" => {
                    let f_start = self.parse_measurement()?;
                    self.expect(&Token::Range)?;
                    let f_stop = self.parse_measurement()?;
                    start_freq = Some(f_start);
                    stop_freq = Some(f_stop);
                }
                "start" => start_freq = Some(self.parse_measurement()?),
                "stop" => stop_freq = Some(self.parse_measurement()?),
                unknown => {
                    return Err(self.error(&format!(
                        "Unknown AC analysis field '{}'. Valid fields: sweep, points, freq, start, stop",
                        unknown
                    )))
                }
            }

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBrace)?;

        let points = points.ok_or_else(|| self.error("AC analysis missing required field 'points: <Count>'"))?;
        let start_freq = start_freq.ok_or_else(|| self.error("AC analysis missing required field 'freq: <Start>..<Stop>'"))?;
        let stop_freq = stop_freq.ok_or_else(|| self.error("AC analysis missing required stop frequency"))?;

        Ok(AcAnalysis {
            scale,
            points,
            start_freq,
            stop_freq,
            span,
        })
    }

    /// Parse Transient Analysis: `{ step: 10ps, stop: 50ns, start: 0s, uic: true }`
    fn parse_tran_analysis(&mut self) -> Result<TranAnalysis, ParseError> {
        let span = self.current_span();
        self.expect(&Token::OpenBrace)?;

        let mut step = None;
        let mut stop = None;
        let mut start = None;
        let mut use_initial_conditions = false;

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let key = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "step" => step = Some(self.parse_measurement()?),
                "stop" => stop = Some(self.parse_measurement()?),
                "start" => start = Some(self.parse_measurement()?),
                "uic" => {
                    let val = self.expect_identifier()?;
                    use_initial_conditions = val.as_str() == "true";
                }
                unknown => {
                    return Err(self.error(&format!(
                        "Unknown transient analysis field '{}'. Valid fields: step, stop, start, uic",
                        unknown
                    )))
                }
            }

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBrace)?;

        let step = step.ok_or_else(|| self.error("Transient analysis missing required field 'step: <Time>'"))?;
        let stop = stop.ok_or_else(|| self.error("Transient analysis missing required field 'stop: <Time>'"))?;

        Ok(TranAnalysis {
            step,
            stop,
            start,
            use_initial_conditions,
            span,
        })
    }

    /// Parse Small-Signal Noise Analysis
    fn parse_noise_analysis(&mut self) -> Result<NoiseAnalysis, ParseError> {
        let span = self.current_span();
        self.expect(&Token::OpenBrace)?;

        let mut output_net = None;
        let mut ref_net = None;
        let mut scale = SweepScale::Decade;
        let mut points = None;
        let mut start_freq = None;
        let mut stop_freq = None;

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let key = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "output" => output_net = Some(self.expect_identifier()?),
                "ref" => ref_net = Some(self.expect_identifier()?),
                "sweep" => {
                    let s_id = self.expect_identifier()?;
                    scale = match s_id.as_str() {
                        "dec" => SweepScale::Decade,
                        "oct" => SweepScale::Octave,
                        "lin" => SweepScale::Linear,
                        other => return Err(self.error(&format!("Unknown scale '{}'", other))),
                    };
                }
                "points" => points = Some(self.expect_number()? as u32),
                "freq" => {
                    let f_start = self.parse_measurement()?;
                    self.expect(&Token::Range)?;
                    let f_stop = self.parse_measurement()?;
                    start_freq = Some(f_start);
                    stop_freq = Some(f_stop);
                }
                unknown => {
                    return Err(self.error(&format!(
                        "Unknown noise analysis field '{}'",
                        unknown
                    )))
                }
            }

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::CloseBrace)?;

        let output_net = output_net.ok_or_else(|| self.error("Noise analysis missing required field 'output: <Net>'"))?;
        let points = points.ok_or_else(|| self.error("Noise analysis missing required field 'points: <Count>'"))?;
        let start_freq = start_freq.ok_or_else(|| self.error("Noise analysis missing required field 'freq: <Start>..<Stop>'"))?;
        let stop_freq = stop_freq.ok_or_else(|| self.error("Noise analysis missing stop frequency"))?;

        Ok(NoiseAnalysis {
            output_net,
            ref_net,
            scale,
            points,
            start_freq,
            stop_freq,
            span,
        })
    }

    /// Parse Generic EDA/SPICE Directive
    fn parse_generic_analysis(&mut self, name: Identifier) -> Result<GenericAnalysis, ParseError> {
        let span = self.current_span();
        let mut parameters = Vec::new();

        if self.check(&Token::OpenBrace) {
            self.advance();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                self.skip_whitespace();
                if self.check(&Token::CloseBrace) || self.is_at_end() {
                    break;
                }

                let key = self.expect_identifier()?;
                self.expect(&Token::Colon)?;
                let val = self.parse_directive_value()?;
                parameters.push((key, val));

                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(&Token::CloseBrace)?;
        }

        Ok(GenericAnalysis {
            name,
            parameters,
            span,
        })
    }

    fn parse_directive_value(&mut self) -> Result<DirectiveValue, ParseError> {
        if self.check(&Token::OpenBrace) {
            self.advance();
            let mut nested = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                self.skip_whitespace();
                if self.check(&Token::CloseBrace) || self.is_at_end() {
                    break;
                }
                let key = self.expect_identifier()?;
                self.expect(&Token::Colon)?;
                let val = self.parse_directive_value()?;
                nested.push((key, val));
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(&Token::CloseBrace)?;
            return Ok(DirectiveValue::Nested(nested));
        }

        if let Ok(m) = self.parse_measurement() {
            return Ok(DirectiveValue::Measure(m));
        }
        if let Ok(num) = self.expect_number() {
            return Ok(DirectiveValue::Number(num));
        }
        if let Ok(s) = self.expect_string() {
            return Ok(DirectiveValue::StringLit(s));
        }
        if let Ok(id) = self.expect_identifier() {
            return Ok(DirectiveValue::Ident(id));
        }

        Err(self.error("Expected measurement, number, string, identifier, or nested block in directive value"))
    }

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
                    let voltage = self.parse_measurement()?;
                    self.expect(&Token::To)?;
                    let pin = self.parse_pin_reference()?;
                    TestActionType::Apply { voltage, pin }
                }
                "short" => {
                    let from = self.parse_pin_reference()?;
                    self.expect(&Token::To)?;
                    let to = self.parse_pin_reference()?;
                    TestActionType::Short { from, to }
                }
                "wait" => {
                    let duration = self.parse_measurement()?;
                    TestActionType::Wait { duration }
                }
                unknown => {
                    return Err(self.error(&format!("Unknown test action: '{}'", unknown)));
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

    fn parse_test_assertions(&mut self) -> Result<Vec<TestAssertion>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut assertions = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let start_pos = self.current_span().start;
            let pin = self.parse_pin_reference()?;

            let condition = if self.check(&Token::LessThan) {
                self.advance();
                TestCondition::LessThan(self.parse_measurement()?)
            } else if self.check(&Token::GreaterThan) {
                self.advance();
                TestCondition::GreaterThan(self.parse_measurement()?)
            } else if self.check(&Token::Equals) {
                self.advance();
                if self.check(&Token::Equals) {
                    self.advance();
                }
                TestCondition::Equals(self.parse_measurement()?)
            } else {
                return Err(self.error("Expected comparison operator (<, >, or =) in test assertion"));
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
