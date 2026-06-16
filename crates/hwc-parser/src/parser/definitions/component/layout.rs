//! Layout block parsing (shape, pin_positions, pad_shapes, internal_pours)

use super::super::super::error::{span_to_source_span, ParseError};
use crate::ast::*;
use crate::lexer::{Span, Token};
use compact_str::CompactString;

impl super::super::super::Parser {
    pub(super) fn parse_layout_block(&mut self) -> Result<LayoutBlock, ParseError> {
        let start_pos = self.current_span().start;
        let mut shape = None;
        let mut pin_positions = rustc_hash::FxHashMap::default();
        let mut pad_shapes = rustc_hash::FxHashMap::default();
        let mut internal_pours = Vec::new();
        let mut standoff = None;

        // eprintln!("[DEBUG] parse_layout_block starting, current token: {:?}, position: {}/{}", self.current().map(|s| &s.token), self.current, self.tokens.len());

        let mut loop_count = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            loop_count += 1;
            if loop_count > 100 {
                // eprintln!("[DEBUG] Layout block loop exceeded 100 iterations!");
                break;
            }
            // eprintln!("[DEBUG] Layout block loop iteration {}, token: {:?}, position: {}/{}", loop_count, self.current().map(|s| &s.token), self.current, self.tokens.len());
            // Skip whitespace and comments
            if self.check(&Token::Newline)
                || self.check(&Token::Indent)
                || matches!(
                    self.current().map(|s| &s.token),
                    Some(Token::BlockComment(_))
                )
            {
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                break;
            }

            // Sprint 2.2: Check for 'add pour' statements (internal component geometry)
            if self.check(&Token::Add) {
                // eprintln!("[DEBUG] Found 'add' token, parsing internal pour");
                match self.parse_component_internal_pour() {
                    Ok(pour) => {
                        internal_pours.push(pour);
                        continue;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            let key_str = self.expect_identifier()?;
            self.expect(&Token::Colon)?;

            match key_str.as_str() {
                "shape" => {
                    shape = Some(self.parse_shape_expression()?);
                    self.skip_until_newline();
                }
                "pin_positions" => {
                    pin_positions = self.parse_pin_positions_block()?;
                }
                "pad_shapes" => {
                    pad_shapes = self.parse_pad_shapes_block()?;
                }
                "standoff" => {
                    standoff = Some(self.parse_expression()?);
                    self.skip_until_newline();
                }
                _ => {
                    // Skip unknown layout properties
                    self.skip_until_newline();
                }
            }
        }

        // Consume the dedent that ends the layout block
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(LayoutBlock {
            shape: shape.map(|s: String| s.into()),
            pin_positions,
            pad_shapes,
            internal_pours, // Sprint 2.2: Parsed internal pours from layout block
            standoff,       // v0.1.7: Parsed stand-off height
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse shape expression: Rectangle(2.0mm, 1.25mm, 0.5mm)
    fn parse_shape_expression(&mut self) -> Result<String, ParseError> {
        let shape_name = self.expect_identifier_string()?;
        let mut shape_str = shape_name.clone();
        // eprintln!("[DEBUG] Parsing shape, name: {}", shape_name);

        // Check if there are parameters
        if self.check(&Token::OpenParen) {
            // eprintln!("[DEBUG] Found opening paren for shape parameters");
            shape_str.push('(');
            self.advance(); // consume (

            // Capture everything until closing paren WITH BOUNDARY CHECKS
            let mut depth = 1;
            let mut iter_count = 0;
            while depth > 0 && !self.is_at_end() {
                iter_count += 1;
                if iter_count > 50 {
                    // eprintln!("[DEBUG] Shape param loop exceeded 50 iterations, breaking!");
                    return Err(self.error("Shape parameter parsing exceeded iteration limit"));
                }

                // eprintln!("[DEBUG] Shape param loop iter {}, depth: {}, token: {:?}", iter_count, depth, self.current().map(|s| &s.token));

                if let Some(spanned) = self.current() {
                    match &spanned.token {
                        Token::OpenParen => {
                            shape_str.push('(');
                            depth += 1;
                            self.advance();
                        }
                        Token::CloseParen => {
                            depth -= 1;
                            // eprintln!("[DEBUG] Found close paren, depth now: {}", depth);
                            if depth > 0 {
                                shape_str.push(')');
                            }
                            self.advance();
                        }
                        Token::Comma => {
                            shape_str.push_str(", ");
                            self.advance();
                        }
                        Token::Measurement(m) => {
                            shape_str.push_str(&format!("{}{}", m.value, m.unit));
                            self.advance();
                        }
                        Token::Integer(n) => {
                            shape_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Float(f) => {
                            shape_str.push_str(&f.to_string());
                            self.advance();
                        }
                        Token::Identifier(id) => {
                            shape_str.push_str(id);
                            self.advance();
                        }
                        Token::Newline | Token::Dedent => {
                            // eprintln!("[DEBUG] Hit Newline/Dedent, depth: {}", depth);
                            // CRITICAL BOUNDARY CHECK: If we hit a line boundary
                            // while still inside parentheses (depth > 0), error out
                            if depth > 0 {
                                return Err(self.error(
                                    "Unclosed parenthesis in shape definition - expected ')'",
                                ));
                            }
                            break;
                        }
                        _ => {
                            // eprintln!("[DEBUG] Unexpected token in shape params, skipping");
                            // For other unexpected tokens, skip them
                            self.advance();
                        }
                    }
                } else {
                    break;
                }
            }
            shape_str.push(')');
            // eprintln!("[DEBUG] Shape parsing complete: {}", shape_str);
        }

        Ok(shape_str)
    }

    /// Parse pin_positions block
    fn parse_pin_positions_block(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, PinPosition>, ParseError> {
        let mut pin_positions = rustc_hash::FxHashMap::default();

        // eprintln!("[DEBUG] Parsing pin_positions block");
        // Expect newline and indent for pin_positions block
        if self.check(&Token::Newline) {
            // eprintln!("[DEBUG] Consuming newline after pin_positions:");
            self.advance();
        }
        if self.check(&Token::Indent) {
            // eprintln!("[DEBUG] Consuming indent for pin_positions block");
            self.advance();
        }

        // Parse pin positions
        let mut pin_loop_count = 0;
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            pin_loop_count += 1;
            if pin_loop_count > 100 {
                // eprintln!("[DEBUG] Pin positions loop exceeded 100 iterations, breaking!");
                break;
            }
            // eprintln!("[DEBUG] Pin positions loop iteration {}, current token: {:?}, position: {}/{}", pin_loop_count, self.current().map(|s| &s.token), self.current, self.tokens.len());

            // Skip whitespace and comments
            if self.check(&Token::Newline)
                || self.check(&Token::Indent)
                || matches!(
                    self.current().map(|s| &s.token),
                    Some(Token::BlockComment(_))
                )
                || matches!(self.current().map(|s| &s.token), Some(Token::DocComment(_)))
            {
                // eprintln!("[DEBUG] Skipping whitespace/comment in pin_positions");
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                // eprintln!("[DEBUG] Found dedent, breaking pin_positions loop");
                break;
            }

            // Check if we're back to a top-level key
            if let Some(spanned) = self.current() {
                if matches!(spanned.token, Token::Identifier(ref id) if id == "shape" || id == "pin_positions" || id == "pad_shapes")
                {
                    // eprintln!("[DEBUG] Found top-level key, breaking pin_positions loop");
                    break;
                }
            }

            // v0.1.7: Handle for loops inside pin_positions
            // Syntax: `for i in 0..7:\n    P[i] at [i * 2.5mm, 0mm, 0mm]`
            if self.check(&Token::For) {
                self.parse_pin_positions_for_loop(&mut pin_positions)?;
                continue;
            }

            let pin_name = self.parse_pin_reference_string()?;

            if self.check(&Token::At) {
                self.advance();

                // Parse measurement list [Xmm, Ymm] or [Xmm, Ymm, Zmm]
                self.expect(&Token::OpenBracket)?;

                let x_meas = self.parse_measurement()?;
                self.expect(&Token::Comma)?;
                let y_meas = self.parse_measurement()?;

                let z_meas = if self.check(&Token::Comma) {
                    self.advance();
                    Some(self.parse_measurement()?)
                } else {
                    None
                };

                self.expect(&Token::CloseBracket)?;

                // Convert measurements to millimeters (canonical unit for pin positions)
                let x_mm = Self::measurement_to_mm(&x_meas);
                let y_mm = Self::measurement_to_mm(&y_meas);
                let z_mm = z_meas.as_ref().map(Self::measurement_to_mm);

                pin_positions.insert(
                    pin_name.into(),
                    PinPosition {
                        x: x_mm,
                        y: y_mm,
                        z: z_mm,
                    },
                );

                // Skip any inline comments after pin position
                self.skip_whitespace();
            }
        }

        // Consume the dedent at the end of pin_positions block
        if self.check(&Token::Dedent) {
            self.advance();
        }

        // v0.1.6: If we just consumed a dedent, we might be ending the pin_positions block
        // but still be inside the layout block. We need to skip potential newlines/indents
        // to find the next layout property (like pad_shapes).
        self.skip_whitespace();

        Ok(pin_positions)
    }

    /// Convert a Measurement to millimeters (canonical unit for pin positions)
    fn measurement_to_mm(m: &Measurement) -> f64 {
        match m.unit {
            Unit::Millimeter => m.value,
            Unit::Micrometer => m.value / 1000.0,
            Unit::Centimeter => m.value * 10.0,
            Unit::Nanometer => m.value / 1_000_000.0,
            _ => panic!(
                "measurement_to_mm: cannot convert {:?} to millimeters (not a length unit)",
                m.unit
            ),
        }
    }

    /// Parse a for loop inside pin_positions block and expand it inline.
    ///
    /// Syntax:
    /// ```hardware
    /// for i in 0..7:
    ///     P[i] at [i * 2.5mm, 0mm, 0mm]
    /// ```
    ///
    /// The range is inclusive (Ruby-style): `0..7` produces i = 0,1,2,3,4,5,6,7 (8 items).
    /// Pin names and coordinate expressions can reference the loop variable.
    fn parse_pin_positions_for_loop(
        &mut self,
        pin_positions: &mut rustc_hash::FxHashMap<CompactString, PinPosition>,
    ) -> Result<(), ParseError> {
        // Parse for loop header: for variable in start..end:
        self.expect(&Token::For)?;
        let variable = self.expect_identifier_string()?;
        self.expect(&Token::In)?;
        let range_start = self.expect_number()? as usize;
        self.expect(&Token::Range)?;
        let range_end = self.expect_number()? as usize;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        // Parse body entries as templates (pin_name string + coordinate expressions)
        #[derive(Clone)]
        struct PinTemplate {
            name_template: String,
            x_expr: Expression,
            y_expr: Expression,
            z_expr: Option<Expression>,
        }

        let mut templates: Vec<PinTemplate> = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            // Skip whitespace and comments
            if self.check(&Token::Newline)
                || self.check(&Token::Indent)
                || matches!(
                    self.current().map(|s| &s.token),
                    Some(Token::BlockComment(_))
                )
                || matches!(self.current().map(|s| &s.token), Some(Token::DocComment(_)))
            {
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                break;
            }

            // Parse pin name as raw template string (preserving loop variable references like D[i])
            let name_template = self.parse_pin_name_template()?;

            if self.check(&Token::At) {
                self.advance();
                self.expect(&Token::OpenBracket)?;

                // Parse coordinates as expressions (to handle loop variable arithmetic)
                let x_expr = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let y_expr = self.parse_expression()?;
                let z_expr = if self.check(&Token::Comma) {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                self.expect(&Token::CloseBracket)?;

                templates.push(PinTemplate {
                    name_template,
                    x_expr,
                    y_expr,
                    z_expr,
                });

                self.skip_whitespace();
            }
        }

        self.expect(&Token::Dedent)?;

        // Expand the for loop: substitute variable and evaluate expressions
        // INCLUSIVE range (Ruby-style): start..end includes both endpoints
        for i in range_start..=range_end {
            let mut context: EvaluationContext = rustc_hash::FxHashMap::default();
            context.insert(variable.clone().into(), i as i64);

            for template in &templates {
                // Substitute variable in pin name: D[i] → D[0], P[i] → P[0], etc.
                let pin_name = template
                    .name_template
                    .replace(&format!("[{}]", variable), &format!("[{}]", i));

                // Evaluate coordinate expressions with the loop variable bound
                let x_val = self.evaluate_pin_pos_expr(&template.x_expr, &context)?;
                let y_val = self.evaluate_pin_pos_expr(&template.y_expr, &context)?;
                let z_val = template
                    .z_expr
                    .as_ref()
                    .map(|e| self.evaluate_pin_pos_expr(e, &context))
                    .transpose()?;

                pin_positions.insert(
                    pin_name.into(),
                    PinPosition {
                        x: x_val,
                        y: y_val,
                        z: z_val,
                    },
                );
            }
        }

        Ok(())
    }

    /// Parse a pin name as a raw template string, allowing loop variable references.
    ///
    /// Unlike `parse_pin_reference_string()` which requires integer array indices,
    /// this method accepts any token inside brackets: `D[i]`, `P[i+1]`, etc.
    /// The result is a string template that can be expanded during for-loop unrolling.
    ///
    /// Syntax: `Identifier` or `Identifier[Expression]`
    fn parse_pin_name_template(&mut self) -> Result<String, ParseError> {
        let name = self.expect_identifier_string()?;

        if self.check(&Token::OpenBracket) {
            self.advance(); // consume [

            // Read tokens inside brackets until CloseBracket
            let mut index_str = String::new();
            let mut depth = 1;
            while !self.is_at_end() && depth > 0 {
                if let Some(spanned) = self.current() {
                    match &spanned.token {
                        Token::OpenBracket => {
                            index_str.push('[');
                            depth += 1;
                            self.advance();
                        }
                        Token::CloseBracket => {
                            depth -= 1;
                            if depth > 0 {
                                index_str.push(']');
                            }
                            self.advance();
                        }
                        Token::Identifier(id) => {
                            index_str.push_str(id);
                            self.advance();
                        }
                        Token::Integer(n) => {
                            index_str.push_str(&n.to_string());
                            self.advance();
                        }
                        Token::Float(f) => {
                            index_str.push_str(&f.to_string());
                            self.advance();
                        }
                        Token::Plus => {
                            index_str.push('+');
                            self.advance();
                        }
                        Token::Hyphen => {
                            index_str.push('-');
                            self.advance();
                        }
                        Token::Asterisk => {
                            index_str.push('*');
                            self.advance();
                        }
                        Token::Slash => {
                            index_str.push('/');
                            self.advance();
                        }
                        Token::Percent => {
                            index_str.push('%');
                            self.advance();
                        }
                        _ => {
                            return Err(ParseError::UnexpectedToken {
                                span: span_to_source_span(&spanned.span),
                                expected: "pin index expression".into(),
                                found: format!("{}", spanned.token).into(),
                            });
                        }
                    }
                } else {
                    break;
                }
            }

            Ok(format!("{}[{}]", name, index_str))
        } else {
            Ok(name)
        }
    }

    /// Evaluate an expression for a pin position, converting the result to millimeters.
    ///
    /// Handles:
    /// - Plain measurements: `2.5mm` → 2.5
    /// - Arithmetic with loop variables: `i * 2.5mm` → 0.0, 2.5, 5.0, ...
    /// - Pure numbers (assumed mm): `0mm` → 0.0
    fn evaluate_pin_pos_expr(
        &self,
        expr: &Expression,
        context: &EvaluationContext,
    ) -> Result<f64, ParseError> {
        let value = expr.evaluate(context).map_err(|e| ParseError::General {
            message: format!("Failed to evaluate pin position expression: {}", e).into(),
            span: span_to_source_span(&expr.span()),
        })?;

        match value {
            Value::Measurement { value, unit } => {
                // Convert to mm
                let mm = match unit {
                    Unit::Millimeter => value,
                    Unit::Micrometer => value / 1000.0,
                    Unit::Centimeter => value * 10.0,
                    _ => value,
                };
                Ok(mm)
            }
            Value::Number(n) => Ok(n as f64),
            Value::Float(f) => Ok(f),
            _ => Err(ParseError::General {
                message: "Pin position expression must evaluate to a number or measurement".into(),
                span: span_to_source_span(&expr.span()),
            }),
        }
    }

    /// Parse pad_shapes block
    fn parse_pad_shapes_block(
        &mut self,
    ) -> Result<rustc_hash::FxHashMap<CompactString, String>, ParseError> {
        let mut pad_shapes = rustc_hash::FxHashMap::default();

        // Expect newline and indent for pad_shapes block
        if self.check(&Token::Newline) {
            self.advance();
        }
        if self.check(&Token::Indent) {
            self.advance();
        }

        // Parse pad shapes: pin_name: Shape(params)
        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) || self.check(&Token::Indent) {
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                break;
            }

            // Check if we're back to a top-level key
            if let Some(spanned) = self.current() {
                if matches!(spanned.token, Token::Identifier(ref id) if id == "shape" || id == "pin_positions" || id == "pad_shapes")
                {
                    break;
                }
            }

            let pin_name = self.parse_pin_reference_string()?;
            self.expect(&Token::Colon)?;

            // Parse pad shape (e.g., Circle(0.5mm), Rectangle(1mm, 0.8mm))
            let pad_shape_str = self.parse_shape_expression()?;
            pad_shapes.insert(pin_name.into(), pad_shape_str);

            // Skip any inline comments after pad shape
            self.skip_whitespace();
        }

        // Consume dedent after pad_shapes block
        if self.check(&Token::Dedent) {
            self.advance();
        }

        // Skip whitespace after block
        self.skip_whitespace();

        Ok(pad_shapes)
    }
}
