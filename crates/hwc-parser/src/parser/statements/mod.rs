//! HardwareScript v0.3.0 Statement and Block Parser

mod assert_stmt;
mod blocks;
mod control_flow;
mod let_stmt;
mod logic;
mod reg_region;
mod route;
mod types;

use crate::ast::{AssignmentOperator, Statement};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse a single statement
    pub fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let start_pos = self.current_span().start;

        if self.check(&Token::Let) {
            self.parse_let_statement(start_pos)
        } else if self.check(&Token::If) {
            self.parse_if_statement(start_pos)
        } else if self.check(&Token::For) {
            self.parse_for_statement(start_pos)
        } else if self.check(&Token::Break) {
            self.parse_break_statement(start_pos)
        } else if self.check(&Token::Continue) {
            self.parse_continue_statement(start_pos)
        } else if self.check(&Token::Match) {
            self.parse_match_statement(start_pos)
        } else if self.check(&Token::Return) {
            self.parse_return_statement(start_pos)
        } else if self.check(&Token::Assert) {
            self.parse_assert_statement(start_pos)
        } else if self.check(&Token::Route) {
            self.parse_route_statement(start_pos)
        } else if self.check(&Token::Logic) {
            let logic_blk = self.parse_logic_block(start_pos)?;
            Ok(Statement::Logic(logic_blk))
        } else if self.check(&Token::Reg) {
            let reg_decl = self.parse_reg_decl(start_pos)?;
            Ok(Statement::Reg(reg_decl))
        } else if self.check(&Token::Region) {
            let region_decl = self.parse_region_decl(start_pos)?;
            Ok(Statement::Region(region_decl))
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
}
