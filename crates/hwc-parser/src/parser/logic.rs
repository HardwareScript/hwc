//! Parser for logic synthesis blocks (v0.4.0)
//!
//! Implements parsing for `logic:` blocks using Rust-like syntax.
//! Reference: Logic Synthesis Specification v0.4.0

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse a logic definition: `define logic name:`
    pub fn parse_logic_definition(&mut self) -> Result<LogicDefinition, ParseError> {
        let start = self.current_span();

        // 'define' and 'logic' already consumed by parse_definition
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        // Parse statements directly (no nested 'logic:' keyword)
        let mut statements = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            statements.push(self.parse_logic_statement()?);
        }

        self.expect(&Token::Dedent)?;

        let logic_span = Span::new(start.start, self.previous_span().end);
        let logic_block = LogicBlock {
            statements,
            span: logic_span,
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicDefinition {
            name,
            logic_block,
            span,
        })
    }

    /// Parse an enum definition: `enum Name: Variant1, Variant2 = 0x1`
    pub fn parse_enum(&mut self) -> Result<EnumDefinition, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Enum)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut variants = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let variant_start = self.current_span();
            let variant_name = self.expect_identifier_string()?;

            let value = if self.check(&Token::Equals) {
                self.advance();
                // Parse integer literal (including hex like 0x1)
                if let Some(Token::Integer(n)) = self.current().map(|t| &t.token) {
                    let val = *n;
                    self.advance();
                    Some(val)
                } else {
                    return Err(self.error("Expected integer value after '=' in enum variant"));
                }
            } else {
                None
            };

            // Optional comma
            if self.check(&Token::Comma) {
                self.advance();
            }

            self.consume_statement_end()?;

            let variant_span = Span::new(variant_start.start, self.previous_span().end);

            variants.push(EnumVariant {
                name: variant_name.into(),
                value,
                span: variant_span,
            });
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(EnumDefinition {
            name,
            variants,
            span,
        })
    }

    /// Parse a struct definition: `struct Name: field1[8], field2[4]`
    pub fn parse_struct(&mut self) -> Result<StructDefinition, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Struct)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut fields = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            let field_start = self.current_span();
            let field_name = self.expect_identifier_string()?;

            self.expect(&Token::OpenBracket)?;
            let width = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;

            self.consume_statement_end()?;

            let field_span = Span::new(field_start.start, self.previous_span().end);

            fields.push(StructField {
                name: field_name.into(),
                width,
                span: field_span,
            });
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(StructDefinition { name, fields, span })
    }

    /// Parse a logic block: `logic:`
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

            // Parse statement with error recovery
            match self.parse_logic_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    collector.report(e);
                    // Synchronize to next statement (skip to next line)
                    self.sync_to_next_logic_statement();

                    // Check if we should stop collecting errors
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

    /// Synchronize to the next logic statement after an error
    /// Skips tokens until we find a statement keyword or newline
    fn sync_to_next_logic_statement(&mut self) {
        while let Some(token) = self.current() {
            match &token.token {
                // Statement keywords - stop here
                Token::Let | Token::If | Token::Match => break,
                // End of block - stop here
                Token::Dedent | Token::Newline => {
                    self.advance();
                    break;
                }
                // Keep skipping
                _ => self.advance(),
            }
        }
    }

    /// Parse a logic statement
    fn parse_logic_statement(&mut self) -> Result<LogicStatement, ParseError> {
        // Check for 'pass' keyword (as identifier) - treat as empty/no-op
        if let Some(Token::Identifier(name)) = self.current().map(|t| &t.token) {
            if name == "pass" {
                self.advance();
                self.consume_statement_end()?;
                // Return a dummy if statement that does nothing
                // This is a bit of a hack, but pass is really just for empty blocks
                // In a real implementation, we might want a Pass statement variant
                return Ok(LogicStatement::If {
                    condition: LogicExpression::Boolean {
                        value: false,
                        span: self.previous_span(),
                    },
                    then_block: BlockOrExpr::Pass(self.previous_span()),
                    else_block: None,
                    span: self.previous_span(),
                });
            }
        }

        if self.check(&Token::Let) {
            self.parse_let_statement()
        } else if self.check(&Token::If) {
            self.parse_if_statement()
        } else {
            // Try to parse as assignment or expression statement
            self.parse_assignment_or_expression_statement()
        }
    }

    /// Parse assignment or expression statement
    /// This handles both `x = expr` and standalone expressions like `CpuState.Decode`
    fn parse_assignment_or_expression_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let _start = self.current_span();

        // Try to parse as assignment target first
        let checkpoint = self.current;

        // Check if this looks like an assignment: identifier (. or [)? =
        let is_assignment = if let Some(Token::Identifier(..)) = self.current().map(|t| &t.token) {
            self.advance();
            let has_accessor = self.check(&Token::Dot) || self.check(&Token::OpenBracket);
            if has_accessor {
                self.advance();
                if matches!(
                    self.current().map(|t| &t.token),
                    Some(Token::Identifier(..)) | Some(Token::Integer(..))
                ) {
                    self.advance();
                }
                if self.check(&Token::CloseBracket) {
                    self.advance();
                }
            }
            let result = self.check(&Token::Equals);
            self.current = checkpoint; // Reset
            result
        } else {
            false
        };

        if is_assignment {
            self.parse_assignment_statement()
        } else {
            // Parse as bare expression statement (tail expression)
            let expression = self.parse_logic_expression()?;
            self.consume_statement_end()?;

            // Return as a bare expression statement (no fake _expr variable.into())
            Ok(LogicStatement::Expression(expression))
        }
    }

    /// Parse let statement: `let x = A + B` or `let mut result[16] = 0`
    fn parse_let_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Let)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.expect_identifier_string()?;

        let width = if self.check(&Token::OpenBracket) {
            self.advance();
            let w = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Some(w)
        } else {
            None
        };

        self.expect(&Token::Equals)?;

        let expression = self.parse_logic_expression()?;

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::Let {
            mutable,
            name: name.into(),
            width,
            expression,
            span,
        })
    }

    /// Parse assignment: `result = A + B` or `state.next = Value`
    fn parse_assignment_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        let target = self.parse_assignment_target()?;

        self.expect(&Token::Equals)?;

        let expression = self.parse_logic_expression()?;

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::Assignment {
            target,
            expression,
            span,
        })
    }

    /// Parse assignment target
    fn parse_assignment_target(&mut self) -> Result<AssignmentTarget, ParseError> {
        let start = self.current_span();
        let name = self.expect_identifier_string()?;

        if self.check(&Token::Dot) {
            self.advance();
            let field = self.expect_identifier()?;

            if field.as_str() == "next" {
                let span = Span::new(start.start, self.previous_span().end);
                Ok(AssignmentTarget::RegisterNext {
                    name: name.into(),
                    span,
                })
            } else {
                Err(self.error(&format!(
                    "Invalid field '{}' in assignment target. Only '.next' is allowed for registers",
                    field
                )))
            }
        } else if self.check(&Token::OpenBracket) {
            self.advance();
            let range = self.parse_range()?;
            self.expect(&Token::CloseBracket)?;
            let span = Span::new(start.start, self.previous_span().end);
            Ok(AssignmentTarget::Slice {
                name: name.into(),
                range,
                span,
            })
        } else {
            let span = Span::new(start.start, self.previous_span().end);
            Ok(AssignmentTarget::Variable {
                name: name.into(),
                span,
            })
        }
    }

    /// Parse if statement: `if condition: ...`
    fn parse_if_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;

        let condition = self.parse_logic_expression()?;

        self.expect(&Token::Colon)?;

        let then_block = self.parse_block_or_expr()?;

        // Skip newlines before checking for else
        while self.check(&Token::Newline) {
            self.advance();
        }

        let else_block = if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::Colon)?;
            Some(self.parse_block_or_expr()?)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicStatement::If {
            condition,
            then_block,
            else_block,
            span,
        })
    }

    /// Parse block or expression
    fn parse_block_or_expr(&mut self) -> Result<BlockOrExpr, ParseError> {
        // Safely skip leading blank lines AND comments
        self.skip_whitespace();

        // 1. Handle inline 'pass'
        if let Some(Token::Identifier(name)) = self.current().map(|t| &t.token) {
            if name == "pass" {
                let span = self.current_span();
                self.advance(); // consume 'pass'
                let _ = self.consume_statement_end(); // safely consume trailing newline if any
                return Ok(BlockOrExpr::Pass(span));
            }
        }

        // 2. Block Parsing (Indented)
        if self.check(&Token::Indent) {
            self.advance(); // consume Indent

            let mut statements = Vec::new();

            // Linear LL(1) parsing - NO speculation, NO rewinding!
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                // Safely skip any empty lines and comments between statements!
                self.skip_whitespace();

                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                statements.push(self.parse_logic_statement()?);
            }

            self.expect(&Token::Dedent)?;

            // 3. Post-Parsing AST Transformation
            if statements.len() == 1 {
                match &statements[0] {
                    LogicStatement::Expression(expr) => {
                        return Ok(BlockOrExpr::Expression(expr.clone()));
                    }
                    LogicStatement::If {
                        condition,
                        then_block,
                        else_block,
                        span,
                    } => {
                        let else_expr = else_block.clone().unwrap_or(BlockOrExpr::Pass(*span));

                        let expr = LogicExpression::If {
                            condition: Box::new(condition.clone()),
                            then_expr: Box::new(then_block.clone()),
                            else_expr: Box::new(else_expr),
                            span: *span,
                        };
                        return Ok(BlockOrExpr::Expression(expr));
                    }
                    _ => {}
                }
            }

            return Ok(BlockOrExpr::Block(statements));
        }

        // 4. Inline parsing (Same line expression)
        let expr = self.parse_logic_expression()?;
        Ok(BlockOrExpr::Expression(expr))
    }

    /// Parse range: `8` or `7..0`
    fn parse_range(&mut self) -> Result<Range, ParseError> {
        let first = self.expect_integer()?;

        if self.check(&Token::Range) {
            self.advance();
            let second = self.expect_integer()?;
            Ok(Range::Slice {
                high: first,
                low: second,
            })
        } else {
            Ok(Range::Single(first))
        }
    }

    /// Parse logic expression
    pub fn parse_logic_expression(&mut self) -> Result<LogicExpression, ParseError> {
        self.parse_logic_expression_prec(0)
    }

    /// Parse logic expression with precedence climbing
    fn parse_logic_expression_prec(&mut self, min_prec: u8) -> Result<LogicExpression, ParseError> {
        let mut left = self.parse_logic_postfix()?;

        loop {
            // Check for reserved symbol '%' used as operator
            if self.check(&Token::Percent) {
                let span = self.current_span();
                return Err(crate::parser::error::error_percent_as_operator(&span));
            }

            // Try to parse a valid operator
            let op = match self.try_parse_logic_operator() {
                Some(op) => op,
                None => break, // No more operators
            };

            let prec = op.precedence();
            if prec < min_prec {
                break;
            }

            self.advance(); // consume operator

            let right = self.parse_logic_expression_prec(prec + 1)?;

            let span = Span::new(left.span().start, right.span().end);

            left = LogicExpression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                span,
            };
        }

        // Check for 'as' cast
        if self.check(&Token::As) {
            self.advance();
            let target_type = self.expect_identifier_string()?;
            let span = Span::new(left.span().start, self.previous_span().end);
            left = LogicExpression::Cast {
                expression: Box::new(left),
                target_type: target_type.into(),
                span,
            };
        }

        Ok(left)
    }

    /// Parse postfix expressions (field access, array access)
    fn parse_logic_postfix(&mut self) -> Result<LogicExpression, ParseError> {
        let mut expr = self.parse_logic_primary()?;

        loop {
            if self.check(&Token::Dot) {
                self.advance();
                let field = self.expect_identifier_string()?;
                let span = Span::new(expr.span().start, self.previous_span().end);
                expr = LogicExpression::FieldAccess {
                    base: Box::new(expr),
                    field: field.into(),
                    span,
                };
            } else if self.check(&Token::OpenBracket) {
                self.advance();
                let range = self.parse_range()?;
                self.expect(&Token::CloseBracket)?;
                let span = Span::new(expr.span().start, self.previous_span().end);
                expr = LogicExpression::ArrayAccess {
                    base: Box::new(expr),
                    range,
                    span,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Try to parse a logic operator from current token
    fn try_parse_logic_operator(&self) -> Option<LogicOperator> {
        self.current().and_then(|t| match &t.token {
            Token::Plus => Some(LogicOperator::Add),
            Token::Hyphen => Some(LogicOperator::Subtract),
            Token::Asterisk => Some(LogicOperator::Multiply),
            Token::Slash => Some(LogicOperator::Divide),
            Token::Mod => Some(LogicOperator::Modulo),
            Token::Ampersand | Token::And => Some(LogicOperator::BitwiseAnd),
            Token::Pipe | Token::Or => Some(LogicOperator::BitwiseOr),
            Token::Xor => Some(LogicOperator::BitwiseXor),
            Token::ShiftLeft => Some(LogicOperator::ShiftLeft),
            Token::ShiftRight => Some(LogicOperator::ShiftRight),
            Token::Equals => Some(LogicOperator::Equal),
            Token::NotEquals => Some(LogicOperator::NotEqual),
            Token::LessThan => Some(LogicOperator::LessThan),
            Token::GreaterThan => Some(LogicOperator::GreaterThan),
            Token::LessThanOrEqual => Some(LogicOperator::LessThanOrEqual),
            Token::GreaterThanOrEqual => Some(LogicOperator::GreaterThanOrEqual),
            _ => None,
        })
    }

    /// Parse unary expression: `!A`, `not Enable` (v0.1.6)
    fn parse_unary(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        // Check for unary operator
        let operator = if self.check(&Token::Exclamation) || self.check(&Token::Not) {
            self.advance(); // consume the operator
            LogicUnaryOperator::Not
        } else {
            return Err(self.error("Expected unary operator (! or not)"));
        };

        // Parse the operand (recursively handles nested unary operators)
        let operand = self.parse_logic_postfix()?;
        let span = Span::new(start.start, operand.span().end);

        Ok(LogicExpression::Unary {
            operator,
            operand: Box::new(operand),
            span,
        })
    }

    /// Parse primary logic expression
    fn parse_logic_primary(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        if let Some(token) = self.current() {
            match &token.token {
                // Unary operators (v0.1.6)
                Token::Exclamation | Token::Not => self.parse_unary(),

                // Literals
                Token::Integer(n) => {
                    let value = *n;
                    self.advance();
                    Ok(LogicExpression::Literal { value, span: start })
                }

                // Boolean literals
                Token::True => {
                    self.advance();
                    Ok(LogicExpression::Boolean {
                        value: true,
                        span: start,
                    })
                }

                Token::False => {
                    self.advance();
                    Ok(LogicExpression::Boolean {
                        value: false,
                        span: start,
                    })
                }

                // Grouped expression
                Token::OpenParen => {
                    self.advance();
                    let expr = self.parse_logic_expression()?;
                    self.expect(&Token::CloseParen)?;
                    let span = Span::new(start.start, self.previous_span().end);
                    Ok(LogicExpression::Grouped {
                        expression: Box::new(expr),
                        span,
                    })
                }

                // Match expression
                Token::Match => self.parse_match_expression(),

                // If expression (inline)
                Token::If => self.parse_if_inline_expression(),

                // Register initialization
                Token::RegisterInit => self.parse_register_init(),

                // Bundle
                Token::OpenBracket => self.parse_bundle_expression(),

                // Variable reference
                Token::Identifier(_) => {
                    let name = self.expect_identifier_string()?;
                    let span = Span::new(start.start, self.previous_span().end);
                    Ok(LogicExpression::Variable {
                        name: name.into(),
                        span,
                    })
                }

                _ => Err(self.error(&format!("Expected logic expression, found {}", token.token))),
            }
        } else {
            Err(self.error("Unexpected end of input in logic expression"))
        }
    }

    /// Parse match expression: `match OpCode: 0x0: A, 0x1: B, else: 0`
    fn parse_match_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Match)?;

        let selector = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut arms = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            arms.push(self.parse_match_arm()?);
        }

        // Match expression must clean up its own indentation!
        // This dedent closes the match block and returns to the parent level
        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::Match {
            selector,
            arms,
            span,
        })
    }

    /// Parse match arm: `0x0: A` or `else: 0` or `CpuState.Fetch: ...`
    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.current_span();

        // Parse pattern: else, integer literal, or EnumType.Variant
        let pattern = if self.check(&Token::Else) {
            self.advance();
            MatchPattern::Else
        } else if let Some(Token::Integer(n)) = self.current().map(|t| &t.token) {
            // Direct integer literal (including hex like 0x1)
            let value = *n;
            self.advance();
            MatchPattern::Literal(value)
        } else {
            // Enum variant: EnumType.Variant
            let enum_name = self.expect_identifier_string()?;
            self.expect(&Token::Dot)?;
            let variant = self.expect_identifier_string()?;
            MatchPattern::EnumVariant {
                enum_name: enum_name.into(),
                variant,
            }
        };

        self.expect(&Token::Colon)?;

        let body = self.parse_block_or_expr()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(MatchArm {
            pattern,
            body,
            span,
        })
    }

    /// Parse register initialization: `reg(clock: Clk, reset: Rst, init: 0)`
    fn parse_register_init(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::RegisterInit)?;
        self.expect(&Token::OpenParen)?;

        // Parse clock: Expr
        self.expect_identifier_value("clock")?;
        self.expect(&Token::Colon)?;
        let clock = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Comma)?;

        // Parse reset: Expr
        self.expect_identifier_value("reset")?;
        self.expect(&Token::Colon)?;
        let reset = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::Comma)?;

        // Parse init: Expr
        self.expect_identifier_value("init")?;
        self.expect(&Token::Colon)?;
        let init = Box::new(self.parse_logic_expression()?);

        self.expect(&Token::CloseParen)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::RegisterInit {
            clock,
            reset,
            init,
            span,
        })
    }

    /// Parse if expression: `if Enable: A else: B` or multi-line blocks
    fn parse_if_inline_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;
        let condition = Box::new(self.parse_logic_expression()?);
        self.expect(&Token::Colon)?;

        let then_expr = Box::new(self.parse_block_or_expr()?);

        // Lookahead for 'else'. Skip all whitespace AND comments safely!
        let bookmark = self.current;
        self.skip_whitespace();

        let else_expr = if self.check(&Token::Else) {
            self.advance(); // Consume 'else'
            self.expect(&Token::Colon)?;
            Box::new(self.parse_block_or_expr()?)
        } else {
            // No 'else' found. Cleanly backtrack so we don't steal from outer statements.
            self.current = bookmark;
            Box::new(BlockOrExpr::Expression(LogicExpression::Literal {
                value: 0,
                span: self.current_span(),
            }))
        };

        let span = Span::new(start.start, self.previous_span().end);
        Ok(LogicExpression::If {
            condition,
            then_expr,
            else_expr,
            span,
        })
    }

    /// Parse bundle expression: `[A[8], B[8]]` or `[(0 * 12), Value[4]]`
    fn parse_bundle_expression(&mut self) -> Result<LogicExpression, ParseError> {
        let start = self.current_span();

        self.expect(&Token::OpenBracket)?;

        let mut items = Vec::new();

        loop {
            // Strip any comments inside the array before parsing the item
            self.skip_whitespace();

            // Allow trailing commas and empty arrays cleanly
            if self.check(&Token::CloseBracket) {
                break;
            }

            let item_start = self.current_span();
            let expr = self.parse_logic_expression()?;

            // Check if this is a duplication pattern: (value * count)
            let item = if let LogicExpression::Grouped { expression, .. } = &expr {
                if let LogicExpression::Binary {
                    left,
                    operator,
                    right,
                    ..
                } = expression.as_ref()
                {
                    if matches!(operator, LogicOperator::Multiply) {
                        if let LogicExpression::Literal { value, .. } = right.as_ref() {
                            let span = Span::new(item_start.start, self.previous_span().end);
                            BundleItem::Duplication {
                                value: left.clone(),
                                count: *value as usize,
                                span,
                            }
                        } else {
                            BundleItem::Expression(expr)
                        }
                    } else {
                        BundleItem::Expression(expr)
                    }
                } else {
                    BundleItem::Expression(expr)
                }
            } else {
                BundleItem::Expression(expr)
            };

            items.push(item);

            // Clear whitespace before checking for the comma
            self.skip_whitespace();

            if !self.check(&Token::Comma) {
                break;
            }
            self.advance(); // consume comma
        }

        self.skip_whitespace(); // clear any comments before the closing bracket
        self.expect(&Token::CloseBracket)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(LogicExpression::Bundle { items, span })
    }

    /// Expect a specific identifier value
    pub(super) fn expect_identifier_value(&mut self, expected: &str) -> Result<(), ParseError> {
        if let Some(token) = self.current() {
            if let Token::Identifier(name) = &token.token {
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
    use crate::lexer::Lexer;

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
