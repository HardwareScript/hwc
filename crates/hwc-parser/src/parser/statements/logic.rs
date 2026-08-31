use crate::ast::{
    AssignmentOperator, LogicBlock, LogicElseBranch, LogicStatement,
};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    pub fn parse_logic_block(&mut self, start_pos: usize) -> Result<LogicBlock, ParseError> {
        self.expect_token(&Token::Logic, "Expected 'logic'")?;
        self.expect_token(&Token::OpenBrace, "Expected '{' to start logic block")?;

        let mut statements = Vec::new();
        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let stmt = self.parse_logic_statement()?;
            statements.push(stmt);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close logic block")?;
        Ok(LogicBlock {
            statements,
            span: crate::ast::Span::new(start_pos, close_span.end),
        })
    }

    pub fn parse_logic_statement(&mut self) -> Result<LogicStatement, ParseError> {
        let start_pos = self.current_span().start;

        if self.check(&Token::Reg) {
            let reg_decl = self.parse_reg_decl(start_pos)?;
            Ok(LogicStatement::Reg(reg_decl))
        } else if self.check(&Token::If) {
            self.parse_logic_if_statement(start_pos)
        } else {
            let expr = self.parse_expression()?;

            let assign_op = match self.current().map(|t| &t.token) {
                Some(Token::Equals) => Some(AssignmentOperator::Assign),
                Some(Token::PlusEquals) => Some(AssignmentOperator::PlusAssign),
                Some(Token::MinusEquals) => Some(AssignmentOperator::MinusAssign),
                Some(Token::StarEquals) => Some(AssignmentOperator::StarAssign),
                Some(Token::SlashEquals) => Some(AssignmentOperator::SlashAssign),
                Some(Token::PercentEquals) => Some(AssignmentOperator::PercentAssign),
                _ => None,
            };

            if let Some(op) = assign_op {
                self.advance();
                let value = self.parse_expression()?;
                let end_pos = value.span().end;
                Ok(LogicStatement::Assignment {
                    target: expr,
                    operator: op,
                    value,
                    span: crate::ast::Span::new(start_pos, end_pos),
                })
            } else {
                let span = expr.span();
                Ok(LogicStatement::Expression {
                    expression: expr,
                    span,
                })
            }
        }
    }

    fn parse_logic_if_statement(&mut self, start_pos: usize) -> Result<LogicStatement, ParseError> {
        self.expect_token(&Token::If, "Expected 'if'")?;
        let condition = self.parse_expression()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for logic if block")?;
        let mut then_block = Vec::new();
        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }
            then_block.push(self.parse_logic_statement()?);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }
        let close_then = self.expect_token(&Token::CloseBrace, "Expected '}' after logic if block")?;

        let else_branch = if self.check(&Token::Else) {
            self.advance();
            if self.check(&Token::If) {
                let nested_if = self.parse_logic_if_statement(self.current_span().start)?;
                Some(LogicElseBranch::ElseIf(Box::new(nested_if)))
            } else {
                self.expect_token(&Token::OpenBrace, "Expected '{' for logic else block")?;
                let mut else_stmts = Vec::new();
                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    while self.check(&Token::Semicolon) {
                        self.advance();
                    }
                    if self.check(&Token::CloseBrace) || self.is_at_end() {
                        break;
                    }
                    else_stmts.push(self.parse_logic_statement()?);
                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }
                }
                self.expect_token(&Token::CloseBrace, "Expected '}' after logic else block")?;
                Some(LogicElseBranch::Block(else_stmts))
            }
        } else {
            None
        };

        let end_pos = match &else_branch {
            Some(LogicElseBranch::ElseIf(s)) => s.span().end,
            Some(LogicElseBranch::Block(_)) => self.previous_span().end,
            None => close_then.end,
        };

        Ok(LogicStatement::If {
            condition,
            then_block,
            else_branch,
            span: crate::ast::Span::new(start_pos, end_pos),
        })
    }
}
