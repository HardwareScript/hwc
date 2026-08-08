//! Module layout block and statement parsing

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::parser::error::{span_to_source_span, ParseError};

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse module layout block: `layout ModuleName:`
    pub(in crate::parser) fn parse_module_layout_block(
        &mut self,
    ) -> Result<ModuleLayoutBlock<'ast>, ParseError> {
        let start_pos = self.current_span().start;

        // v0.1.6: 'layout' is now an identifier
        self.expect_identifier_string()?; // consume 'layout'
        let module_instance = self.expect_identifier_string()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut statements = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();

            if self.check(&Token::For) {
                statements.push(self.parse_layout_for_loop()?);
            } else if self.check(&Token::If) {
                statements.push(self.parse_layout_if_conditional()?);
            } else if !self.check(&Token::Dedent) {
                statements.push(LayoutStatement::Placement(self.parse_layout_placement()?));
            }
        }

        self.expect(&Token::Dedent)?;

        let end_pos = self.previous_span().end;

        Ok(ModuleLayoutBlock {
            module_instance: module_instance.into(),
            statements,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse a single component placement in a layout block
    /// Returns arena-allocated reference for zero-copy AST
    pub(in crate::parser) fn parse_layout_placement(
        &mut self,
    ) -> Result<&'ast ModuleInternalPlacement, ParseError> {
        let start_pos = self.current_span().start;

        // Parse component name
        let component_name = self.expect_identifier_string()?;

        // Parse optional array index expression: Component[i] or Component[i-1]
        let array_index = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let index = self.parse_layout_array_index()?;
            self.expect(&Token::CloseBracket)?;
            Some(index)
        } else {
            None
        };

        self.expect(&Token::At)?;
        let position = self.parse_coordinate()?;
        self.expect(&Token::Newline)?;

        // Arena-allocate and return reference
        let placement = self.arena.alloc(ModuleInternalPlacement {
            component_name: component_name.into(),
            array_index,
            position,
            span: Span::new(start_pos, self.previous_span().end),
        });

        Ok(placement)
    }

    /// Parse for loop in layout block
    pub(in crate::parser) fn parse_layout_for_loop(
        &mut self,
    ) -> Result<LayoutStatement<'ast>, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::For)?;
        let variable = self.expect_identifier_string()?;
        self.expect(&Token::In)?;
        let start = self.expect_number()? as usize;
        self.expect(&Token::Range)?;
        let end = self.expect_number()? as usize;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut body = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();

            if self.check(&Token::For) {
                body.push(self.parse_layout_for_loop()?);
            } else if self.check(&Token::If) {
                body.push(self.parse_layout_if_conditional()?);
            } else if !self.check(&Token::Dedent) {
                body.push(LayoutStatement::Placement(self.parse_layout_placement()?));
            }
        }

        self.expect(&Token::Dedent)?;

        Ok(LayoutStatement::For {
            variable: variable.into(),
            start,
            end,
            body,
            span: Span::new(start_pos, self.previous_span().end),
        })
    }

    /// Parse if conditional in layout block
    pub(in crate::parser) fn parse_layout_if_conditional(
        &mut self,
    ) -> Result<LayoutStatement<'ast>, ParseError> {
        let start_pos = self.current_span().start;

        self.expect(&Token::If)?;
        let condition = self.parse_layout_condition()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut then_body = Vec::new();

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            self.skip_whitespace();

            if self.check(&Token::For) {
                then_body.push(self.parse_layout_for_loop()?);
            } else if self.check(&Token::If) {
                then_body.push(self.parse_layout_if_conditional()?);
            } else if !self.check(&Token::Dedent) {
                then_body.push(LayoutStatement::Placement(self.parse_layout_placement()?));
            }
        }

        self.expect(&Token::Dedent)?;

        let else_body = if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::Colon)?;
            self.expect(&Token::Newline)?;
            self.expect(&Token::Indent)?;

            let mut else_statements = Vec::new();

            while !self.is_at_end() && !self.check(&Token::Dedent) {
                self.skip_whitespace();

                if self.check(&Token::For) {
                    else_statements.push(self.parse_layout_for_loop()?);
                } else if self.check(&Token::If) {
                    else_statements.push(self.parse_layout_if_conditional()?);
                } else if !self.check(&Token::Dedent) {
                    else_statements
                        .push(LayoutStatement::Placement(self.parse_layout_placement()?));
                }
            }

            self.expect(&Token::Dedent)?;
            Some(else_statements)
        } else {
            None
        };

        Ok(LayoutStatement::If {
            condition,
            then_body,
            else_body,
            span: Span::new(start_pos, self.previous_span().end),
        })
    }

    /// Parse array index for layout blocks (duplicated from module parser)
    pub(super) fn parse_layout_array_index(&mut self) -> Result<crate::ArrayIndex, ParseError> {
        use crate::ArrayIndex;

        if let Some(Token::Integer(n)) = self.current().map(|t| &t.token) {
            let value = *n as usize;
            self.advance();
            Ok(ArrayIndex::Literal(value))
        } else if let Some(Token::Identifier(name)) = self.current().map(|t| &t.token) {
            let var_name = name.clone();
            self.advance();

            // Check for arithmetic: i+1, i-1
            if self.check(&Token::Plus) || self.check(&Token::Hyphen) {
                let op = if self.check(&Token::Plus) {
                    self.advance();
                    crate::ArithmeticOp::Add
                } else {
                    self.advance();
                    crate::ArithmeticOp::Subtract
                };

                let right = Box::new(self.parse_layout_array_index()?);
                Ok(ArrayIndex::Arithmetic {
                    left: Box::new(ArrayIndex::Variable(var_name)),
                    op,
                    right,
                })
            } else {
                Ok(ArrayIndex::Variable(var_name))
            }
        } else {
            let span = self.current_span();
            Err(ParseError::UnexpectedToken {
                expected: "integer or identifier".into(),
                found: format!("{:?}", self.current().map(|t| &t.token)).into(),
                span: span_to_source_span(&span),
            })
        }
    }

    /// Parse condition for layout if statements (duplicated from module parser)
    pub(super) fn parse_layout_condition(&mut self) -> Result<crate::Condition, ParseError> {
        use crate::Condition;

        let left = self.parse_layout_array_index()?;

        let condition = if self.check(&Token::Equals) {
            self.advance();
            let right = self.parse_layout_array_index()?;
            Condition::Equals { left, right }
        } else if self.check(&Token::NotEquals) {
            self.advance();
            let right = self.parse_layout_array_index()?;
            Condition::NotEquals { left, right }
        } else if self.check(&Token::LessThan) {
            self.advance();
            let right = self.parse_layout_array_index()?;
            Condition::LessThan { left, right }
        } else if self.check(&Token::GreaterThan) {
            self.advance();
            let right = self.parse_layout_array_index()?;
            Condition::GreaterThan { left, right }
        } else {
            let span = self.current_span();
            return Err(ParseError::UnexpectedToken {
                expected: "comparison operator (==, !=, <, >)".into(),
                found: format!("{:?}", self.current().map(|t| &t.token)).into(),
                span: span_to_source_span(&span),
            });
        };

        Ok(condition)
    }
}
