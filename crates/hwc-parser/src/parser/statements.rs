//! HardwareScript v0.3.0 Statement and Block Parser

use crate::ast::{
    AssignmentOperator, Block, ElseBranch, MatchArm, Pattern, Statement, TypeExpr,
};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse a block of statements delimited by `{ ... }`
    pub fn parse_block(&mut self) -> Result<Block, ParseError> {
        let open_span = self.expect_token(&Token::OpenBrace, "Expected '{' to begin block")?;
        let mut statements = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            // Skip optional semicolons
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    // Recover within block
                    return Err(e);
                }
            }

            // Optional semicolon after statement
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close block")?;
        Ok(Block {
            statements,
            span: crate::ast::Span::new(open_span.start, close_span.end),
        })
    }

    /// Parse a single statement
    pub fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let start_pos = self.current_span().start;

        if self.check(&Token::Let) {
            self.parse_let_statement(start_pos)
        } else if self.check(&Token::If) {
            self.parse_if_statement(start_pos)
        } else if self.check(&Token::For) {
            self.parse_for_statement(start_pos)
        } else if self.check(&Token::Match) {
            self.parse_match_statement(start_pos)
        } else if self.check(&Token::Return) {
            self.parse_return_statement(start_pos)
        } else if self.check(&Token::Assert) {
            self.parse_assert_statement(start_pos)
        } else if self.check(&Token::Route) {
            self.parse_route_statement(start_pos)
        } else {
            // Expression or Assignment statement
            let expr = self.parse_expression()?;

            // Check if this is an assignment statement: target (= | += | -= | *= | /=) value
            let assign_op = match self.current().map(|t| &t.token) {
                Some(Token::Equals) => Some(AssignmentOperator::Assign),
                Some(Token::PlusEquals) => Some(AssignmentOperator::PlusAssign),
                Some(Token::MinusEquals) => Some(AssignmentOperator::MinusAssign),
                Some(Token::StarEquals) => Some(AssignmentOperator::StarAssign),
                Some(Token::SlashEquals) => Some(AssignmentOperator::SlashAssign),
                _ => None,
            };

            if let Some(op) = assign_op {
                self.advance(); // consume assignment operator
                let value = self.parse_expression()?;
                let end_pos = value.span().end;
                Ok(Statement::Assignment {
                    target: expr,
                    operator: op,
                    value,
                    span: crate::ast::Span::new(start_pos, end_pos),
                })
            } else {
                let end_pos = expr.span().end;
                Ok(Statement::Expression {
                    expression: expr,
                    span: crate::ast::Span::new(start_pos, end_pos),
                })
            }
        }
    }

    /// Parse `let (mut)? x (: Type)? = expr;`
    fn parse_let_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Let, "Expected 'let'")?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let ident = self.expect_identifier()?;
        let name: CompactString = ident.name.as_str().into();

        let type_annotation = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect_token(&Token::Equals, "Expected '=' in let statement")?;
        let value = self.parse_expression()?;
        let end_pos = value.span().end;

        Ok(Statement::Let {
            mutable,
            name,
            type_annotation,
            value,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    /// Parse `if cond { ... } else { ... }`
    fn parse_if_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::If, "Expected 'if'")?;
        let condition = self.parse_expression()?;
        let then_block = self.parse_block()?;

        let else_branch = if self.check(&Token::Else) {
            self.advance(); // consume `else`
            if self.check(&Token::If) {
                let else_if = self.parse_if_statement(self.current_span().start)?;
                Some(ElseBranch::ElseIf(Box::new(else_if)))
            } else {
                let else_block = self.parse_block()?;
                Some(ElseBranch::Block(else_block))
            }
        } else {
            None
        };

        let end_pos = match &else_branch {
            Some(ElseBranch::ElseIf(stmt)) => stmt.span().end,
            Some(ElseBranch::Block(blk)) => blk.span.end,
            None => then_block.span.end,
        };

        Ok(Statement::If {
            condition,
            then_block,
            else_branch,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    /// Parse `for i in 0..10 { ... }` or `for k, v in items { ... }`
    fn parse_for_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::For, "Expected 'for'")?;

        let first_ident = self.expect_identifier()?;
        let mut variables = vec![CompactString::from(first_ident.name.as_str())];

        if self.check(&Token::Comma) {
            self.advance();
            let second_ident = self.expect_identifier()?;
            variables.push(CompactString::from(second_ident.name.as_str()));
        }

        self.expect_token(&Token::In, "Expected 'in' in for loop")?;
        let iterable = self.parse_expression()?;
        let body = self.parse_block()?;
        let end_pos = body.span.end;

        Ok(Statement::For {
            variables,
            iterable,
            body,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    /// Parse `return (expr)?;`
    fn parse_return_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Return, "Expected 'return'")?;

        let (value, end_pos) = if !self.check(&Token::Semicolon)
            && !self.check(&Token::CloseBrace)
            && !self.is_at_end()
        {
            let expr = self.parse_expression()?;
            let end = expr.span().end;
            (Some(expr), end)
        } else {
            (None, self.previous_span().end)
        };

        Ok(Statement::Return {
            value,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    /// Parse `assert(condition, "message"?, args...);`
    fn parse_assert_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Assert, "Expected 'assert'")?;
        self.expect_token(&Token::OpenParen, "Expected '(' after assert")?;

        let condition = self.parse_expression()?;
        let mut message = None;
        let mut args = Vec::new();

        if self.check(&Token::Comma) {
            self.advance();
            if let Some(Token::String(msg)) = self.current().map(|t| &t.token) {
                message = Some(msg.clone());
                self.advance();

                while self.check(&Token::Comma) {
                    self.advance();
                    args.push(self.parse_expression()?);
                }
            } else {
                args.push(self.parse_expression()?);
            }
        }

        let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close assert")?;

        Ok(Statement::Assert {
            condition,
            message,
            args,
            span: crate::ast::Span::new(start_pos, close_span.end),
        })
    }

    /// Parse `route from to (with intent: Identifier | Block)?`
    fn parse_route_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Route, "Expected 'route'")?;
        let from = self.parse_expression()?;
        self.expect_token(&Token::To, "Expected 'to' in route statement")?;
        let to = self.parse_expression()?;

        let mut intent = None;
        let mut body = None;

        if self.check(&Token::With) {
            self.advance();
            self.expect_token(&Token::Intent, "Expected 'intent' after with")?;
            self.expect_token(&Token::Colon, "Expected ':' after intent")?;
            let intent_ident = self.expect_identifier()?;
            intent = Some(intent_ident.name.as_str().into());
        }

        if self.check(&Token::OpenBrace) {
            body = Some(self.parse_block()?);
        }

        let end_pos = if let Some(blk) = &body {
            blk.span.end
        } else {
            to.span().end
        };

        Ok(Statement::Route {
            from,
            to,
            intent,
            body,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }

    /// Parse match statement: `match target { pattern => { ... }, ... }`
    fn parse_match_statement(&mut self, start_pos: usize) -> Result<Statement, ParseError> {
        self.expect_token(&Token::Match, "Expected 'match'")?;
        let target = self.parse_expression()?;
        self.expect_token(&Token::OpenBrace, "Expected '{' after match target")?;

        let mut arms = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let arm_start = self.current_span().start;

            // 1. Parse pattern: either `_` or an expression (like `TapType.P_Sub`)
            let pattern = if let Some(Token::Identifier(id)) = self.current().map(|t| &t.token) {
                if id.as_str() == "_" {
                    let span = self.current_span();
                    self.advance();
                    Pattern::Wildcard { span }
                } else {
                    Pattern::Expr(self.parse_expression()?)
                }
            } else {
                Pattern::Expr(self.parse_expression()?)
            };

            // 2. Expect `=>` fat arrow
            self.expect_token(&Token::FatArrow, "Expected '=>' after match pattern")?;

            // 3. Parse block body `{ ... }`
            let body = self.parse_block()?;

            let arm_end = body.span.end;

            // Optional trailing comma between match arms
            if self.check(&Token::Comma) {
                self.advance();
            }

            arms.push(MatchArm {
                pattern,
                body,
                span: crate::ast::Span::new(arm_start, arm_end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close match")?;

        Ok(Statement::Match {
            target,
            arms,
            span: crate::ast::Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a type expression: `Type`, `Array[Type]`, `(Type1, Type2)`, `fn(T1) -> T2`
    pub fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let start_pos = self.current_span().start;

        if self.check(&Token::Fn) {
            self.advance();
            self.expect_token(&Token::OpenParen, "Expected '(' for function type parameters")?;
            let mut params = Vec::new();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                params.push(self.parse_type_expr()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseParen, "Expected ')'")?;

            let return_type = if self.check(&Token::Arrow) {
                self.advance();
                Some(Box::new(self.parse_type_expr()?))
            } else {
                None
            };

            let end_pos = if let Some(ret) = &return_type {
                ret.span().end
            } else {
                self.previous_span().end
            };

            Ok(TypeExpr::Function {
                params,
                return_type,
                span: crate::ast::Span::new(start_pos, end_pos),
            })
        } else if self.check(&Token::OpenParen) {
            self.advance();
            let mut elements = Vec::new();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                elements.push(self.parse_type_expr()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            let close_span = self.expect_token(&Token::CloseParen, "Expected ')' to close tuple type")?;
            Ok(TypeExpr::Tuple {
                elements,
                span: crate::ast::Span::new(start_pos, close_span.end),
            })
        } else {
            let ident = self.expect_identifier()?;
            let type_name: CompactString = ident.name.as_str().into();
            let mut type_args = Vec::new();

            if self.check(&Token::OpenBracket) {
                self.advance();
                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    type_args.push(self.parse_type_expr()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close generic type arguments")?;
                Ok(TypeExpr::Named {
                    name: type_name,
                    type_args,
                    span: crate::ast::Span::new(start_pos, close_span.end),
                })
            } else {
                Ok(TypeExpr::Named {
                    name: type_name,
                    type_args: Vec::new(),
                    span: crate::ast::Span::new(start_pos, self.previous_span().end),
                })
            }
        }
    }
}
