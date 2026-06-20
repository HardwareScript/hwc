use crate::ast::expr::{BinOp, Expr, UnaryOp};
use crate::ast::*;
use crate::lexer::Token;

/// Known constants that can appear in geometry expressions
const KNOWN_CONSTANTS: &[&str] = &["PI", "DEG_TO_RAD"];

// STRICT SEMANTIC SEPARATION (Boundary Law):
// Inside geometry blocks (logic blocks): '=' is used for behavioral actions (e.g., `let x = 5`).
// Inside property blocks: ':' is used for declarative facts (e.g., `net: GND`).
// The geometry parser enforces '=' for assignments; property parsers enforce ':' for declarations.

impl crate::parser::Parser {
    pub(in crate::parser::definitions::shape) fn parse_geometry_blocks(
        &mut self,
    ) -> Result<Vec<GeometryBlock>, crate::parser::error::ParseError> {
        let mut blocks = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check_identifier("for") {
                blocks.push(self.parse_geometry_for_loop()?);
                self.skip_whitespace();
                continue;
            }

            if self.check_identifier("let") {
                blocks.push(self.parse_geometry_let()?);
                self.skip_whitespace();
                continue;
            }

            return Err(self.error("Expected 'for', 'let', or 'Point' in geometry block"));
        }

        if blocks.is_empty() {
            return Err(self.error("Geometry block must contain at least one statement"));
        }

        Ok(blocks)
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_for_loop(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        self.expect_identifier_named("for")?;

        let variable = self.expect_identifier()?.name.to_string();
        self.expect_identifier_named("in")?;

        let start = self.parse_geometry_expr()?;

        self.expect(&Token::Range)?;

        let end = self.parse_geometry_expr()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;

        self.skip_whitespace();
        self.expect(&Token::Indent)?;

        let mut body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check_identifier("let") {
                let (name, value) = self.parse_geometry_let_statement()?;
                body.push(GeometryStatement::LetBinding { name, value });
            } else if self.check_identifier("Point") {
                body.push(self.parse_geometry_point_statement()?);
            } else if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    let name = name.clone();
                    // Check if this is a generator call (identifier followed by '(')
                    let pos = self.current;
                    if pos + 1 < self.tokens.len() {
                        if let Token::OpenParen = &self.tokens[pos + 1].token {
                            self.advance(); // consume identifier
                            let args = self.parse_generator_call_args()?;
                            body.push(GeometryStatement::GeneratorCall { name, args });
                            continue;
                        }
                    }
                    return Err(self.error("Expected 'let', 'Point', or generator call in for loop body"));
                } else {
                    return Err(self.error("Expected 'let', 'Point', or generator call in for loop body"));
                }
            } else {
                return Err(self.error("Expected 'let', 'Point', or generator call in for loop body"));
            }
        }

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

    pub(in crate::parser::definitions::shape) fn parse_geometry_let(
        &mut self,
    ) -> Result<GeometryBlock, crate::parser::error::ParseError> {
        let (name, value) = self.parse_geometry_let_statement()?;
        Ok(GeometryBlock::LetBinding { name, value })
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_let_statement(
        &mut self,
    ) -> Result<(String, Expr), crate::parser::error::ParseError> {
        self.expect_identifier_named("let")?;

        let name = self.expect_identifier()?.name.to_string();
        self.expect(&Token::Equals)?;

        let value = self.parse_geometry_expr()?;
        Ok((name, value))
    }

    pub(in crate::parser::definitions::shape) fn parse_geometry_point_statement(
        &mut self,
    ) -> Result<GeometryStatement, crate::parser::error::ParseError> {
        self.expect_identifier_named("Point")?;
        self.expect(&Token::OpenParen)?;

        self.expect_identifier_named("x")?;
        self.expect(&Token::Colon)?;
        let x = self.parse_geometry_expr()?;

        self.expect(&Token::Comma)?;

        self.expect_identifier_named("y")?;
        self.expect(&Token::Colon)?;
        let y = self.parse_geometry_expr()?;

        self.expect(&Token::CloseParen)?;

        Ok(GeometryStatement::Point { x, y })
    }

    /// Parse generator call arguments: (name: expr, name: expr, ...)
    pub(in crate::parser::definitions::shape) fn parse_generator_call_args(
        &mut self,
    ) -> Result<Vec<(String, Expr)>, crate::parser::error::ParseError> {
        self.expect(&Token::OpenParen)?;
        let mut args = Vec::new();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            self.skip_whitespace();
            if self.check(&Token::CloseParen) {
                break;
            }

            let name = self.expect_identifier()?.name.to_string();
            self.expect(&Token::Colon)?;
            let value = self.parse_geometry_expr()?;
            args.push((name, value));

            self.skip_whitespace();
            if self.check(&Token::Comma) {
                self.advance();
            } else if !self.check(&Token::CloseParen) {
                return Err(self.error("Expected ',' or ')' in generator call arguments"));
            }
        }

        self.expect(&Token::CloseParen)?;
        Ok(args)
    }

    pub(super) fn parse_geometry_expr(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        self.parse_geometry_expr_if()
    }

    fn parse_geometry_expr_if(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check_identifier("if") {
            self.advance();
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

    fn parse_geometry_expr_comparison(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
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
            self.advance();
            let right = self.parse_geometry_expr_addition()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_geometry_expr_addition(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
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
                self.advance();
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
                self.advance();
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

    fn parse_geometry_expr_unary(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        if self.check(&Token::Plus) {
            self.advance();
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Pos,
                expr: Box::new(expr),
            })
        } else if self.check(&Token::Hyphen) {
            self.advance();
            let expr = self.parse_geometry_expr_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            })
        } else {
            self.parse_geometry_expr_call()
        }
    }

    fn parse_geometry_expr_call(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
        let atom = self.parse_geometry_expr_atom()?;

        if let Expr::Identifier(ref name) = atom {
            if self.check(&Token::OpenParen) {
                let name = name.clone();
                self.advance();
                let mut args = Vec::new();

                if !self.check(&Token::CloseParen) && !self.is_at_end() {
                    args.push(self.parse_geometry_expr()?);
                    while self.check(&Token::Comma) {
                        self.advance();
                        args.push(self.parse_geometry_expr()?);
                    }
                }

                self.expect(&Token::CloseParen)?;
                return Ok(Expr::Call { name, args });
            }
        }

        Ok(atom)
    }

    fn parse_geometry_expr_atom(&mut self) -> Result<Expr, crate::parser::error::ParseError> {
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
                    // Check for known constants (PI, DEG_TO_RAD)
                    if KNOWN_CONSTANTS.contains(&name.as_str()) {
                        let name = name.clone();
                        self.advance();
                        return Ok(Expr::Constant(name));
                    }
                    let name = name.clone();
                    self.advance();
                    Ok(Expr::Identifier(name))
                }
                Token::OpenParen => {
                    self.advance();
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
}

#[cfg(test)]
mod tests {
    use crate::ast::expr::Expr;
    use crate::ast::{GeometryBlock, GeometryStatement};
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::DiagnosticCollector;

    fn parse_shape(source: &str) -> crate::ast::ShapeDefinition {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let program = parser.parse(&collector);
        assert!(
            !collector.has_errors(),
            "Parse errors: {}",
            collector.summary()
        );
        assert_eq!(program.definitions.len(), 1);
        if let crate::ast::Definition::Shape(shape) =
            program.definitions.into_iter().next().unwrap()
        {
            shape
        } else {
            panic!("Expected shape definition");
        }
    }

    #[test]
    fn test_geometry_point_parses() {
        let source = r#"shape S:
    geometry:
        for i in 0..1:
            Point(x: 1mm, y: 2mm)
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        assert_eq!(geometry.len(), 1);
        if let GeometryBlock::ForLoop {
            variable,
            start,
            end,
            body,
        } = &geometry[0]
        {
            assert_eq!(variable, "i");
            assert_eq!(*start, Expr::Literal(0.0));
            assert_eq!(*end, Expr::Literal(1.0));
            assert_eq!(body.len(), 1);
            if let GeometryStatement::Point { x, y } = &body[0] {
                assert_eq!(*x, Expr::Literal(1.0));
                assert_eq!(*y, Expr::Literal(2.0));
            } else {
                panic!("Expected Point statement");
            }
        } else {
            panic!("Expected ForLoop");
        }
    }

    #[test]
    fn test_geometry_for_loop_with_let_and_point() {
        let source = r#"shape S:
    geometry:
        for i in 0..5:
            let angle = i * 11.25
            Point(x: cos(angle), y: sin(angle))
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        assert_eq!(geometry.len(), 1);
        if let GeometryBlock::ForLoop {
            variable,
            start,
            end,
            body,
        } = &geometry[0]
        {
            assert_eq!(variable, "i");
            assert_eq!(*start, Expr::Literal(0.0));
            assert_eq!(*end, Expr::Literal(5.0));
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected ForLoop");
        }
    }

    #[test]
    fn test_geometry_let_binding_at_block_scope() {
        let source = r#"shape S:
    geometry:
        let angle = 45
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        assert_eq!(geometry.len(), 1);
        if let GeometryBlock::LetBinding { name, .. } = &geometry[0] {
            assert_eq!(name, "angle");
        } else {
            panic!("Expected LetBinding");
        }
    }

    #[test]
    fn test_trig_functions_parse_as_call() {
        let source = r#"shape S:
    geometry:
        for i in 0..1:
            let x = sin(rad)
            let y = cos(rad)
            let z = tan(rad)
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        if let GeometryBlock::ForLoop { body, .. } = &geometry[0] {
            assert_eq!(body.len(), 3);
            if let GeometryStatement::LetBinding { value, .. } = &body[0] {
                assert!(
                    matches!(value, Expr::Call { name, .. } if name == "sin"),
                    "Expected sin() call, got {:?}",
                    value
                );
            } else {
                panic!("Expected LetBinding");
            }
            if let GeometryStatement::LetBinding { value, .. } = &body[1] {
                assert!(
                    matches!(value, Expr::Call { name, .. } if name == "cos"),
                    "Expected cos() call, got {:?}",
                    value
                );
            } else {
                panic!("Expected LetBinding");
            }
            if let GeometryStatement::LetBinding { value, .. } = &body[2] {
                assert!(
                    matches!(value, Expr::Call { name, .. } if name == "tan"),
                    "Expected tan() call, got {:?}",
                    value
                );
            } else {
                panic!("Expected LetBinding");
            }
        } else {
            panic!("Expected ForLoop");
        }
    }

    #[test]
    fn test_constants_pi_and_deg_to_rad() {
        let source = r#"shape S:
    geometry:
        for i in 0..1:
            let x = PI
            let y = DEG_TO_RAD
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        if let GeometryBlock::ForLoop { body, .. } = &geometry[0] {
            assert_eq!(body.len(), 2);
            if let GeometryStatement::LetBinding { value, .. } = &body[0] {
                assert!(
                    matches!(value, Expr::Constant(name) if name == "PI"),
                    "Expected PI constant, got {:?}",
                    value
                );
            } else {
                panic!("Expected LetBinding");
            }
            if let GeometryStatement::LetBinding { value, .. } = &body[1] {
                assert!(
                    matches!(value, Expr::Constant(name) if name == "DEG_TO_RAD"),
                    "Expected DEG_TO_RAD constant, got {:?}",
                    value
                );
            } else {
                panic!("Expected LetBinding");
            }
        } else {
            panic!("Expected ForLoop");
        }
    }

    #[test]
    fn test_generator_call_in_geometry_body() {
        let source = r#"shape S:
    geometry:
        for i in 0..1:
            StarGenerator(points: 24, outer: 5mm, inner: 2mm)
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        if let GeometryBlock::ForLoop { body, .. } = &geometry[0] {
            assert_eq!(body.len(), 1);
            if let GeometryStatement::GeneratorCall { name, args } = &body[0] {
                assert_eq!(name, "StarGenerator");
                assert_eq!(args.len(), 3);
                assert_eq!(args[0].0, "points");
                assert_eq!(args[1].0, "outer");
                assert_eq!(args[2].0, "inner");
            } else {
                panic!("Expected GeneratorCall, got {:?}", body[0]);
            }
        } else {
            panic!("Expected ForLoop");
        }
    }

    #[test]
    fn test_for_loop_with_expr_bounds() {
        let source = r#"shape S:
    geometry:
        let n = 8
        for i in 0..n:
            let angle = i * (PI / n)
            Point(x: cos(angle) * 5mm, y: sin(angle) * 5mm)
"#;
        let shape = parse_shape(source);
        let geometry = shape.geometry.expect("Expected geometry block");
        assert_eq!(geometry.len(), 2);
        if let GeometryBlock::ForLoop { start, end, .. } = &geometry[1] {
            assert_eq!(*start, Expr::Literal(0.0));
            assert!(
                matches!(end, Expr::Identifier(name) if name == "n"),
                "Expected identifier 'n' for loop end, got {:?}",
                end
            );
        } else {
            panic!("Expected ForLoop at index 1");
        }
    }
}
