use rustc_hash::FxHashMap;

use crate::ast::*;
use crate::ast::expr::{BinOp, Expr, UnaryOp};
use crate::lexer::{Span, Token};
use crate::parser::error::span_to_source_span;

// Helper methods for CSG parsing
impl super::super::Parser {
    /// Check if the current token could be the start of a CSG expression
    pub(in super::super) fn check_csg_operator(&self) -> bool {
        // For now, we check if we have an identifier that could be a shape reference
        // followed by a CSG operator
        if let Some(current) = self.current() {
            if let Token::Identifier(_) = &current.token {
                // Check if next token is +, -, or *
                let pos = self.current;
                if pos + 1 < self.tokens.len() {
                    if let Token::Plus | Token::Hyphen | Token::Asterisk = &self.tokens[pos + 1].token {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Parse a CSG expression (Mode C)
    ///
    /// Syntax:
    /// ```hw
    /// Rectangle(width: width / 1.414, height: width / 1.414)
    /// Circle(diameter: width)
    /// sq + (sq rotated 22.5deg) + (sq rotated 45.0deg)
    /// shape_ref - other_shape
    /// shape1 * shape2
    /// ```
    pub(in super::super) fn parse_csg_expression(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        self.parse_csg_term()
    }

    /// Parse a CSG term (handles union, difference, intersection)
    fn parse_csg_term(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        let mut left = self.parse_csg_factor()?;

        // Handle CSG operators: +, -, *
        loop {
            self.skip_whitespace();
            
            if self.check(&Token::Plus) {
                self.advance(); // consume '+'
                let right = self.parse_csg_factor()?;
                left = CsgExpression::Union(Box::new(left), Box::new(right));
            } else if self.check(&Token::Hyphen) {
                // Make sure this is a binary minus, not a unary sign
                // If we're at the start or after an operator, this could be unary
                if self.is_binary_minus() {
                    self.advance(); // consume '-'
                    let right = self.parse_csg_factor()?;
                    left = CsgExpression::Difference(Box::new(left), Box::new(right));
                } else {
                    break;
                }
            } else if self.check(&Token::Asterisk) {
                self.advance(); // consume '*'
                let right = self.parse_csg_factor()?;
                left = CsgExpression::Intersection(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse a CSG factor (primitive or transformed expression)
    fn parse_csg_factor(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        let mut expr = self.parse_csg_atom()?;

        // Handle transformations: rotated, at
        loop {
            self.skip_whitespace();
            
            if self.check_identifier("rotated") {
                self.advance(); // consume 'rotated'
                // Parse rotation value (e.g., 22.5deg)
                let rotation = self.parse_rotation_value()?;
                expr = CsgExpression::Transformed {
                    expr: Box::new(expr),
                    rotation: Some(rotation),
                    translation: None,
                };
            } else if self.check(&Token::At) {
                self.advance(); // consume 'at'
                // Parse translation: [x: expr, y: expr]
                let translation = self.parse_translation()?;
                expr = CsgExpression::Transformed {
                    expr: Box::new(expr),
                    rotation: None,
                    translation: Some(translation),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parse a CSG atom (primitive shape or parenthesized expression)
    fn parse_csg_atom(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        self.skip_whitespace();

        // Check for Rectangle primitive
        if self.check_identifier("Rectangle") {
            self.advance(); // consume 'Rectangle'
            self.expect(&Token::OpenParen)?;
            
            // Parse width: expr
            self.expect_identifier_named("width")?;
            self.expect(&Token::Colon)?;
            let width = self.read_expression_until_comma_or_close()?;
            
            // Expect comma
            self.expect(&Token::Comma)?;
            
            // Parse height: expr
            self.expect_identifier_named("height")?;
            self.expect(&Token::Colon)?;
            let height = self.read_expression_until_comma_or_close()?;
            
            self.expect(&Token::CloseParen)?;
            
            return Ok(CsgExpression::Primitive(CsgPrimitive::Rectangle { width, height }));
        }

        // Check for Circle primitive
        if self.check_identifier("Circle") {
            self.advance(); // consume 'Circle'
            self.expect(&Token::OpenParen)?;
            
            // Parse diameter: expr
            self.expect_identifier_named("diameter")?;
            self.expect(&Token::Colon)?;
            let diameter = self.read_expression_until_comma_or_close()?;
            
            self.expect(&Token::CloseParen)?;
            
            return Ok(CsgExpression::Primitive(CsgPrimitive::Circle { diameter }));
        }

        // Check for parenthesized expression
        if self.check(&Token::OpenParen) {
            self.advance(); // consume '('
            let expr = self.parse_csg_expression()?;
            self.expect(&Token::CloseParen)?;
            return Ok(expr);
        }

        // Check for identifier (shape reference)
        if let Some(current) = self.current() {
            if let Token::Identifier(name) = &current.token {
                let name = name.clone();
                self.advance(); // consume identifier
                return Ok(CsgExpression::Primitive(CsgPrimitive::ShapeRef(name)));
            }
        }

        Err(self.error("Expected CSG expression: Rectangle(...), Circle(...), identifier, or (expr)"))
    }

    /// Parse rotation value (e.g., 22.5deg)
    fn parse_rotation_value(
        &mut self,
    ) -> Result<f64, crate::parser::error::ParseError> {
        self.skip_whitespace();
        
        // Expect a number followed by 'deg'
        if let Some(current) = self.current() {
            match &current.token {
                Token::Float(n) => {
                    let value = *n;
                    self.advance(); // consume float
                    // Check for 'deg' suffix
                    if self.check_identifier("deg") {
                        self.advance(); // consume 'deg'
                        return Ok(value);
                    }
                    return Ok(value);
                }
                Token::Integer(n) => {
                    let value = *n as f64;
                    self.advance(); // consume integer
                    // Check for 'deg' suffix
                    if self.check_identifier("deg") {
                        self.advance(); // consume 'deg'
                        return Ok(value);
                    }
                    return Ok(value);
                }
                Token::Measurement(m) => {
                    // Handle measurement like 22.5deg
                    let value = m.value;
                    self.advance(); // consume measurement
                    return Ok(value);
                }
                _ => {}
            }
        }
        
        Err(self.error("Expected rotation value (e.g., 22.5deg)"))
    }

    /// Parse translation: [x: expr, y: expr]
    fn parse_translation(
        &mut self,
    ) -> Result<(f64, f64), crate::parser::error::ParseError> {
        self.skip_whitespace();
        self.expect(&Token::OpenBracket)?;
        
        // Parse x: expr
        self.expect_identifier_named("x")?;
        self.expect(&Token::Colon)?;
        let x_str = self.read_expression_until_comma_or_close()?;
        let x = x_str.parse::<f64>().map_err(|_| self.error("Expected number for x translation"))?;
        
        // Expect comma
        self.expect(&Token::Comma)?;
        
        // Parse y: expr
        self.expect_identifier_named("y")?;
        self.expect(&Token::Colon)?;
        let y_str = self.read_expression_until_comma_or_close()?;
        let y = y_str.parse::<f64>().map_err(|_| self.error("Expected number for y translation"))?;
        
        self.expect(&Token::CloseBracket)?;
        
        Ok((x, y))
    }

    /// Check if a `let` statement is followed by a CSG primitive (Rectangle, Circle)
    /// This helps distinguish Mode C (CSG with let bindings) from Mode B (parametric blocks)
    fn lookahead_is_csg_let(&self) -> bool {
        // Try to parse: let <identifier> = <CSG primitive>
        // We need to check if after "let" and "=" we have Rectangle or Circle
        
        // Skip current token (should be "let")
        if !self.check_identifier("let") {
            return false;
        }
        
        // Look ahead to find '='
        let mut pos = self.current;
        // Skip "let"
        if pos < self.tokens.len() {
            pos += 1;
        }
        
        // Skip identifier (variable name)
        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Identifier(_) => {
                    pos += 1;
                    break;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }
        
        // Skip '='
        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Equals => {
                    pos += 1;
                    break;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }
        
        // Check if next meaningful token is Rectangle or Circle
        while pos < self.tokens.len() {
            match &self.tokens[pos].token {
                Token::Identifier(name) if name == "Rectangle" || name == "Circle" => {
                    return true;
                }
                Token::DocComment(_) | Token::BlockComment(_) | Token::DocBlock(_) => {
                    pos += 1;
                }
                _ => break,
            }
        }
        
        false
    }

    /// Parse a CSG expression with let bindings
    /// 
    /// Syntax:
    /// ```hw
    /// let sq = Rectangle(width: width / 1.414, height: width / 1.414)
    /// sq + (sq rotated 22.5deg) + (sq rotated 45.0deg) + (sq rotated 67.5deg)
    /// ```
    fn parse_csg_with_let_bindings(
        &mut self,
    ) -> Result<CsgExpression, crate::parser::error::ParseError> {
        // Parse let bindings until we hit the actual CSG expression
        // We build a chain of LetBinding nodes
        
        let mut body = None;
        
        loop {
            self.skip_whitespace();
            
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }
            
            // Check if this is a let binding
            if self.check_identifier("let") {
                self.advance(); // consume 'let'
                
                // Parse variable name
                let var_name = self.expect_identifier()?.name.to_string();
                
                // Consume '='
                self.expect(&Token::Equals)?;
                
                // Parse the expression (Rectangle, Circle, or another expression)
                let expr = self.parse_csg_factor()?;
                
                // Continue to parse the rest
                // The body will be set after we parse the final expression
                // For now, we'll store the binding and continue
                body = Some((var_name, expr));
            } else {
                // This should be the final CSG expression
                break;
            }
        }
        
        // Parse the final CSG expression
        let final_expr = self.parse_csg_term()?;
        
        // If we have a let binding, wrap the final expression
        if let Some((name, value)) = body {
            Ok(CsgExpression::LetBinding {
                name,
                value: Box::new(value),
                body: Box::new(final_expr),
            })
        } else {
            Ok(final_expr)
        }
    }
}

impl super::super::Parser {
    /// Parse shape definition: `shape HexagonalVia(width: Measurement):`
    ///
    /// Syntax:
    /// ```hw
    /// shape HexagonalVia(width: Measurement):
    ///     points:
    ///         - [x: -width / 2, y: 0]
    ///         - [x: -width / 4, y: width * 0.433]
    ///         - [x: width / 4, y: width * 0.433]
    ///         - [x: width / 2, y: 0]
    ///         - [x: width / 4, y: -width * 0.433]
    ///         - [x: -width / 4, y: -width * 0.433]
    /// ```
    pub(in super::super) fn parse_shape(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<ShapeDefinition> {
        let start_pos = self.current_span().start;

        // 1. Consume 'shape' keyword
        if let Err(e) = self.expect(&Token::Shape) {
            collector.report(e);
            return None;
        }

        // 2. Parse shape name
        let name = match self.expect_identifier() {
            Ok(n) => n,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        // 3. Parse optional parameters in parentheses
        let parameters = if self.check(&Token::OpenParen) {
            self.advance(); // consume '('
            let params = match self.parse_shape_parameters() {
                Ok(p) => p,
                Err(e) => {
                    collector.report(e);
                    return None;
                }
            };
            if let Err(e) = self.expect(&Token::CloseParen) {
                collector.report(e);
                return None;
            }
            params
        } else {
            Vec::new()
        };

        // 4. Expect colon, newline, indent
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }
        self.skip_whitespace();
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            return None;
        }

        // 5. Parse 'points:' or 'geometry:' block
        let mut points = Vec::new();
        let mut generator: Option<ShapeGenerator> = None;
        let mut geometry: Option<Vec<GeometryBlock>> = None;
        let mut csg: Option<CsgExpression> = None;

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if let Some(current) = self.current() {
                if let Token::Identifier(field_name) = &current.token {
                    if field_name == "points" {
                        self.advance(); // consume 'points'
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            return None;
                        }
                        if let Err(e) = self.expect(&Token::Newline) {
                            collector.report(e);
                            return None;
                        }
                        self.skip_whitespace();
                        if let Err(e) = self.expect(&Token::Indent) {
                            collector.report(e);
                            return None;
                        }
                        match self.parse_shape_points() {
                            Ok(pts) => points = pts,
                            Err(e) => {
                                collector.report(e);
                                while !self.check(&Token::Dedent) && !self.is_at_end() {
                                    self.advance();
                                }
                            }
                        }
                        if self.check(&Token::Dedent) {
                            self.advance();
                        }
                        continue;
                    }

                    if field_name == "geometry" {
                        self.advance(); // consume 'geometry'
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            return None;
                        }
                        if let Err(e) = self.expect(&Token::Newline) {
                            collector.report(e);
                            return None;
                        }
                        self.skip_whitespace();
                        if let Err(e) = self.expect(&Token::Indent) {
                            collector.report(e);
                            return None;
                        }
                        // Peek at the first token inside geometry block to decide parsing mode
                        // Mode C: Check if this looks like a CSG expression (Rectangle, Circle, or identifier followed by +, -, *)
                        // Mode C also handles: let sq = Rectangle(...) followed by CSG expressions
                        if self.check_identifier("Rectangle")
                            || self.check_identifier("Circle")
                            || self.check_csg_operator()
                        {
                            // Mode C: CSG expressions
                            match self.parse_csg_expression() {
                                Ok(expr) => csg = Some(expr),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else if self.check_identifier("let") && self.lookahead_is_csg_let() {
                            // Mode C: CSG with let binding (e.g., let sq = Rectangle(...))
                            match self.parse_csg_with_let_bindings() {
                                Ok(expr) => csg = Some(expr),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else if self.check_identifier("for") || self.check_identifier("let")
                            || self.check_identifier("Point")
                        {
                            // Mode B: Parametric geometry blocks
                            match self.parse_geometry_blocks() {
                                Ok(blocks) => geometry = Some(blocks),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        } else {
                            // Legacy: procedural generator call (e.g., StarGenerator(...))
                            match self.parse_shape_generator() {
                                Ok(gen) => generator = Some(gen),
                                Err(e) => {
                                    collector.report(e);
                                    while !self.check(&Token::Dedent) && !self.is_at_end() {
                                        self.advance();
                                    }
                                }
                            }
                        }
                        // Consume newline + dedent(s) after geometry block content.
                        // parse_shape_points() exits on Dedent so only needs one advance,
                        // but parse_shape_generator() exits on ')' leaving a Newline.
                        if self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::Dedent) {
                            self.advance();
                        }
                        continue;
                    }
                }
            }

            // Unknown field - skip it
            let field_name = match self.expect_identifier() {
                Ok(n) => n,
                Err(e) => {
                    collector.report(e);
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                    self.skip_whitespace();
                    continue;
                }
            };
            collector.report(self.error(&format!(
                "Unknown shape field: '{}'",
                field_name
            )));
            while !self.is_at_end() && !self.check(&Token::Newline) && !self.check(&Token::Dedent) {
                self.advance();
            }
            self.skip_whitespace();
        }

        // 6. Consume dedent
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        // Validate: must have either points, a geometry generator, geometry blocks, or CSG expression
        if points.is_empty() && generator.is_none() && geometry.is_none() && csg.is_none() {
            collector.report(crate::parser::error::ParseError::General {
                span: span_to_source_span(&Span::new(start_pos, end_pos)),
                message: "Shape definition must have 'points', 'geometry' generator, geometry blocks, or CSG expression".into(),
            });
            return None;
        }

        Some(ShapeDefinition {
            name,
            parameters,
            points,
            generator,
            geometry,
            csg,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse shape parameters: `(width: Measurement)` or `(width: Measurement = 1mm)`
    fn parse_shape_parameters(
        &mut self,
    ) -> Result<Vec<ShapeParameter>, crate::parser::error::ParseError> {
        let mut parameters = Vec::new();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseParen) {
                break;
            }

            let param_name = self.expect_identifier()?;

            if let Err(e) = self.expect(&Token::Colon) {
                return Err(e);
            }

            // Consume the type annotation (identifier or keyword) — we don't store it yet
            // The type is informational; we just need to skip past it
            self.expect_identifier()?;

            // Check for optional default value
            let default_value = if self.check(&Token::Equals) {
                self.advance(); // consume '='
                Some(self.read_expression_string()?)
            } else {
                None
            };

            parameters.push(ShapeParameter {
                name: param_name,
                default_value,
            });

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance(); // consume comma
            } else if !self.check(&Token::CloseParen) {
                return Err(self.error("Expected ',' or ')' in shape parameters"));
            }
        }

        Ok(parameters)
    }

    /// Parse shape points: list of `- [x: expr, y: expr]` entries
    fn parse_shape_points(&mut self) -> Result<Vec<ShapePoint>, crate::parser::error::ParseError> {
        let mut points = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Expect hyphen for list item
            self.expect(&Token::Hyphen)?;

            // Expect opening bracket
            self.expect(&Token::OpenBracket)?;

            // Expect 'x'
            self.expect_identifier_named("x")?;
            self.expect(&Token::Colon)?;

            // Read x expression until comma
            let x_expr = self.read_expression_until(&Token::Comma)?;

            // Expect comma
            self.expect(&Token::Comma)?;

            // Expect 'y'
            self.expect_identifier_named("y")?;
            self.expect(&Token::Colon)?;

            // Read y expression until closing bracket
            let y_expr = self.read_expression_until(&Token::CloseBracket)?;

            // Expect closing bracket
            self.expect(&Token::CloseBracket)?;

            points.push(ShapePoint { x_expr, y_expr });

            self.skip_whitespace();
        }

        if points.is_empty() {
            return Err(self.error("Shape must have at least one point"));
        }

        Ok(points)
    }

    /// Parse a procedural shape generator call within a `geometry:` block.
    ///
    /// Syntax:
    /// ```hw
    /// geometry:
    ///     StarGenerator(points: 16, outer: width / 2, inner: width / 4)
    /// ```
    fn parse_shape_generator(
        &mut self,
    ) -> Result<ShapeGenerator, crate::parser::error::ParseError> {
        // Expect generator name (e.g., StarGenerator)
        let gen_name = self.expect_identifier()?.name;

        // Expect '('
        self.expect(&Token::OpenParen)?;

        // Parse comma-separated key: value parameters
        let mut params = FxHashMap::default();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::CloseParen) {
                break;
            }

            // Expect parameter name
            let param_name = self.expect_identifier()?.name;

            // Expect ':'
            self.expect(&Token::Colon)?;

            // Read the parameter expression until ',' or ')'
            let param_value = self.read_expression_until_comma_or_close()?;

            params.insert(param_name.to_string(), param_value);

            self.skip_whitespace();

            if self.check(&Token::Comma) {
                self.advance(); // consume comma
            } else if !self.check(&Token::CloseParen) {
                return Err(self.error("Expected ',' or ')' in generator parameters"));
            }
        }

        // Expect ')'
        self.expect(&Token::CloseParen)?;

        Ok(ShapeGenerator {
            name: gen_name.to_string(),
            params,
        })
    }

    /// Parse Mode B geometry blocks: for loops, let statements, and Point expressions.
    ///
    /// Syntax:
    /// ```hw
    /// geometry:
    ///     for i in 0..31:
    ///         let angle = i * 11.25deg
    ///         let r = if i mod 2 = 0: width / 2 else: width / 4
    ///         Point(x: r * cos(angle), y: r * sin(angle))
    /// ```
    fn parse_geometry_blocks(
        &mut self,
    ) -> Result<Vec<GeometryBlock>, crate::parser::error::ParseError> {
        let mut blocks = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Check for 'for' loop
            if self.check_identifier("for") {
                blocks.push(self.parse_geometry_for_loop()?);
                self.skip_whitespace();
                continue;
            }

            // Check for 'let' variable declaration
            if self.check_identifier("let") {
                blocks.push(self.parse_geometry_let()?);
                self.skip_whitespace();
                continue;
            }

            // Unknown statement in geometry block
            return Err(self.error(
                "Expected 'for', 'let', or 'Point' in geometry block",
            ));
        }

        if blocks.is_empty() {
            return Err(self.error("Geometry block must contain at least one statement"));
        }

        Ok(blocks)
    }

    /// Parse a for loop inside a geometry block.
    ///
    /// Syntax: `for i in 0..31:`
    fn parse_geometry_for_loop(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        // Consume 'for'
        self.expect_identifier_named("for")?;

        // Parse loop variable name
        let variable = self.expect_identifier()?.name.to_string();

        // Consume 'in'
        self.expect_identifier_named("in")?;

        // Parse start value (integer)
        let start = if let Some(current) = self.current() {
            if let Token::Integer(n) = &current.token {
                let val = *n;
                self.advance();
                val
            } else {
                return Err(self.error("Expected integer for loop start"));
            }
        } else {
            return Err(self.error("Expected integer for loop start"));
        };

        // Consume '..'
        self.expect(&Token::Range)?;

        // Parse end value (integer)
        let end = if let Some(current) = self.current() {
            if let Token::Integer(n) = &current.token {
                let val = *n;
                self.advance();
                val
            } else {
                return Err(self.error("Expected integer for loop end"));
            }
        } else {
            return Err(self.error("Expected integer for loop end"));
        };

        // Consume ':'
        self.expect(&Token::Colon)?;

        // Consume newline
        self.expect(&Token::Newline)?;

        // Consume indent
        self.skip_whitespace();
        self.expect(&Token::Indent)?;

        // Parse loop body statements
        let mut body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Parse body statement
            if self.check_identifier("let") {
                let (name, value) = self.parse_geometry_let_statement()?;
                body.push(GeometryStatement::Variable { name, value });
            } else if self.check_identifier("Point") {
                body.push(self.parse_geometry_point_statement()?);
            } else {
                return Err(self.error(
                    "Expected 'let' or 'Point' in for loop body",
                ));
            }
        }

        // Consume dedent
        if self.check(&Token::Dedent) {
            self.advance();
        }

        if body.is_empty() {
            return Err(self.error("For loop body must contain at least one statement"));
        }

        Ok(GeometryBlock::ForLoop {
            variable,
            start,
            end,
            body,
        })
    }

    /// Parse a let statement inside a geometry block.
    /// Syntax: `let angle = i * 11.25deg`
    fn parse_geometry_let(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        let (name, value) = self.parse_geometry_let_statement()?;
        Ok(GeometryBlock::Variable { name, value })
    }

    /// Parse a let statement (used both at block level and inside for loops).
    /// Returns (name, value) tuple.
    fn parse_geometry_let_statement(
        &mut self,
    ) -> Result<(String, Expr), crate::parser::error::ParseError> {
        // Consume 'let'
        self.expect_identifier_named("let")?;

        // Parse variable name
        let name = self.expect_identifier()?.name.to_string();

        // Consume '='
        self.expect(&Token::Equals)?;

        // Parse the value expression (stops at newline/dedent)
        let value = self.parse_geometry_expr()?;

        Ok((name, value))
    }

    /// Parse a Point(x: expr, y: expr) statement inside a geometry block.
    fn parse_geometry_point_statement(
        &mut self,
    ) -> Result<GeometryStatement, crate::parser::error::ParseError> {
        // Consume 'Point'
        self.expect_identifier_named("Point")?;

        // Consume '('
        self.expect(&Token::OpenParen)?;

        // Expect 'x'
        self.expect_identifier_named("x")?;
        self.expect(&Token::Colon)?;

        // Parse x expression (stops at comma)
        let x = self.parse_geometry_expr()?;

        // Expect comma
        self.expect(&Token::Comma)?;

        // Expect 'y'
        self.expect_identifier_named("y")?;
        self.expect(&Token::Colon)?;

        // Parse y expression (stops at closing paren)
        let y = self.parse_geometry_expr()?;

        // Consume ')'
        self.expect(&Token::CloseParen)?;

        Ok(GeometryStatement::Point { x, y })
    }

    /// Parse an expression into an Expr AST node.
    ///
    /// Grammar (precedence low→high):
    /// ```text
    /// expr       = if_expr
    /// if_expr    = "if" condition ":" expr "else" ":" expr | comparison
    /// comparison = addition (("=" | "!=" | "<" | ">") addition)?
    /// addition   = multiplication (("+" | "-") multiplication)*
    /// multiplication = unary (("*" | "/" | "mod") unary)*
    /// unary      = ("+" | "-") unary | call
    /// call       = IDENT "(" args ")" | atom
    /// atom       = NUMBER | IDENT | "(" expr ")"
    /// ```
    pub(super) fn parse_geometry_expr(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        self.parse_geometry_expr_if()
    }

    fn parse_geometry_expr_if(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check_identifier("if") {
            self.advance(); // consume 'if'
            let cond = self.parse_geometry_expr_comparison()?;
            self.expect(&Token::Colon)?;
            let then_branch = self.parse_geometry_expr()?;
            self.expect_identifier_named("else")?;
            self.expect(&Token::Colon)?;
            let else_branch = self.parse_geometry_expr()?;
            Ok(Expr::If {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            })
        } else {
            self.parse_geometry_expr_comparison()
        }
    }

    fn parse_geometry_expr_comparison(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_addition()?;

        let op = if self.check(&Token::Equals) {
            Some(BinOp::Eq)
        } else if self.check(&Token::NotEquals) {
            Some(BinOp::Ne)
        } else if self.check(&Token::LessThan) {
            Some(BinOp::Lt)
        } else if self.check(&Token::GreaterThan) {
            Some(BinOp::Gt)
        } else {
            None
        };

        if let Some(op) = op {
            self.advance(); // consume operator
            let right = self.parse_geometry_expr_addition()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_geometry_expr_addition(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_multiplication()?;

        loop {
            let op = if self.check(&Token::Plus) {
                Some(BinOp::Add)
            } else if self.check(&Token::Hyphen) {
                Some(BinOp::Sub)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance(); // consume operator
                let right = self.parse_geometry_expr_multiplication()?;
                left = Expr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_geometry_expr_multiplication(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        let mut left = self.parse_geometry_expr_unary()?;

        loop {
            let op = if self.check(&Token::Asterisk) {
                Some(BinOp::Mul)
            } else if self.check(&Token::Slash) {
                Some(BinOp::Div)
            } else if self.check(&Token::Mod) {
                Some(BinOp::Mod)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance(); // consume operator
                let right = self.parse_geometry_expr_unary()?;
                left = Expr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_geometry_expr_unary(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check(&Token::Plus) {
            self.advance(); // consume '+'
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Pos,
                expr: Box::new(expr),
            })
        } else if self.check(&Token::Hyphen) {
            self.advance(); // consume '-'
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            })
        } else {
            self.parse_geometry_expr_call()
        }
    }

    fn parse_geometry_expr_call(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        let atom = self.parse_geometry_expr_atom()?;

        // Check for function call: IDENT followed by '('
        if let Expr::Identifier(ref name) = atom {
            if self.check(&Token::OpenParen) {
                let name = name.clone();
                self.advance(); // consume '('
                let mut args = Vec::new();

                if !self.check(&Token::CloseParen) && !self.is_at_end() {
                    args.push(self.parse_geometry_expr()?);
                    while self.check(&Token::Comma) {
                        self.advance(); // consume ','
                        args.push(self.parse_geometry_expr()?);
                    }
                }

                self.expect(&Token::CloseParen)?;
                return Ok(Expr::Call { name, args });
            }
        }

        Ok(atom)
    }

    fn parse_geometry_expr_atom(
        &mut self,
    ) -> Result<Expr, crate::parser::error::ParseError> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Integer(n) => {
                    let val = *n as f64;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Float(n) => {
                    let val = *n;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Measurement(m) => {
                    let val = m.value;
                    self.advance();
                    Ok(Expr::Literal(val))
                }
                Token::Identifier(name) => {
                    let name = name.clone();
                    self.advance();
                    Ok(Expr::Identifier(name))
                }
                Token::OpenParen => {
                    self.advance(); // consume '('
                    let expr = self.parse_geometry_expr()?;
                    self.expect(&Token::CloseParen)?;
                    Ok(expr)
                }
                Token::True => {
                    self.advance();
                    Ok(Expr::Literal(1.0))
                }
                Token::False => {
                    self.advance();
                    Ok(Expr::Literal(0.0))
                }
                _ => Err(self.error("Expected expression")),
            }
        } else {
            Err(self.error("Expected expression"))
        }
    }

    /// Read an expression string until a comma or closing paren.
    fn read_expression_until_comma_or_close(
        &mut self,
    ) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;
        let mut depth = 0i32;

        while !self.is_at_end() {
            match self.current().map(|t| &t.token) {
                Some(Token::Comma) if depth == 0 => break,
                Some(Token::CloseParen) if depth == 0 => break,
                Some(Token::OpenParen) => {
                    depth += 1;
                }
                Some(Token::CloseParen) => {
                    depth -= 1;
                }
                Some(Token::Newline) | Some(Token::Dedent) => break,
                _ => {}
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    /// Read tokens as a string expression until the given delimiter token.
    ///
    /// Collects the source text of each token (with spaces) until the delimiter is found.
    fn read_expression_until(
        &mut self,
        delimiter: &Token,
    ) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;

        while !self.is_at_end() {
            if self.check(delimiter) {
                break;
            }
            // Stop at newline or dedent as safety
            if self.check(&Token::Newline) || self.check(&Token::Dedent) {
                break;
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    /// Read an expression string (for default values, etc.)
    fn read_expression_string(&mut self) -> Result<String, crate::parser::error::ParseError> {
        let mut expr_parts = Vec::new();
        let mut first = true;

        while !self.is_at_end() {
            // Stop at comma, close paren, newline, or dedent
            if self.check(&Token::Comma)
                || self.check(&Token::CloseParen)
                || self.check(&Token::Newline)
                || self.check(&Token::Dedent)
            {
                break;
            }

            if let Some(current) = self.current() {
                let token_str = self.token_to_string(&current.token);
                if first {
                    expr_parts.push(token_str);
                    first = false;
                } else {
                    expr_parts.push(format!(" {}", token_str));
                }
                self.advance();
            } else {
                break;
            }
        }

        if expr_parts.is_empty() {
            return Err(self.error("Expected expression"));
        }

        Ok(expr_parts.concat())
    }

    /// Convert a token to its string representation for expression storage
    fn token_to_string(&self, token: &Token) -> String {
        match token {
            Token::Identifier(s) => s.clone(),
            Token::Integer(n) => n.to_string(),
            Token::Float(n) => n.to_string(),
            Token::Measurement(m) => format!("{}", m),
            Token::Hyphen => "-".to_string(),
            Token::Plus => "+".to_string(),
            Token::Asterisk => "*".to_string(),
            Token::Slash => "/".to_string(),
            Token::Percent => "%".to_string(),
            Token::OpenParen => "(".to_string(),
            Token::CloseParen => ")".to_string(),
            Token::OpenBracket => "[".to_string(),
            Token::CloseBracket => "]".to_string(),
            Token::Dot => ".".to_string(),
            Token::Colon => ":".to_string(),
            Token::Comma => ",".to_string(),
            Token::Equals => "=".to_string(),
            Token::LessThan => "<".to_string(),
            Token::GreaterThan => ">".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Pipe => "|".to_string(),
            Token::Tilde => "~".to_string(),
            Token::Exclamation => "!".to_string(),
            Token::ShiftLeft => "<<".to_string(),
            Token::ShiftRight => ">>".to_string(),
            Token::LessThanOrEqual => "<=".to_string(),
            Token::GreaterThanOrEqual => ">=".to_string(),
            Token::NotEquals => "!=".to_string(),
            Token::Range => "..".to_string(),
            Token::OpenBrace => "{".to_string(),
            Token::CloseBrace => "}".to_string(),
            // Keyword tokens used in geometry block expressions
            Token::If => "if".to_string(),
            Token::Else => "else".to_string(),
            Token::Mod => "mod".to_string(),
            Token::For => "for".to_string(),
            Token::In => "in".to_string(),
            Token::Let => "let".to_string(),
            Token::True => "true".to_string(),
            Token::False => "false".to_string(),
            Token::And => "and".to_string(),
            Token::Or => "or".to_string(),
            Token::Not => "not".to_string(),
            _ => format!("<{:?}>", token),
        }
    }
}
