use crate::ast::{AssignmentOperator, Block, Statement};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    pub(crate) fn parse_block(&mut self) -> Result<Block, ParseError> {
        let open_span = self.expect_token(&Token::OpenBrace, "Expected '{' to begin block")?;
        let mut statements = Vec::new();
        let mut tail_expr = None;

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            if self.check(&Token::Let)
                || self.check(&Token::If)
                || self.check(&Token::For)
                || self.check(&Token::Break)
                || self.check(&Token::Continue)
                || self.check(&Token::Match)
                || self.check(&Token::Return)
                || self.check(&Token::Assert)
                || self.check(&Token::Route)
                || self.check(&Token::Logic)
                || self.check(&Token::Reg)
                || self.check(&Token::Region)
            {
                let stmt = self.parse_statement()?;
                statements.push(stmt);
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
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
                    let span = crate::ast::Span::new(expr.span().start, value.span().end);
                    statements.push(Statement::Assignment {
                        target: expr,
                        operator: op,
                        value,
                        span,
                    });
                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }
                } else if self.check(&Token::Semicolon) {
                    self.advance();
                    let span = expr.span();
                    statements.push(Statement::Expression {
                        expression: expr,
                        span,
                    });
                } else if self.check(&Token::CloseBrace) {
                    tail_expr = Some(Box::new(expr));
                    break;
                } else {
                    let span = expr.span();
                    statements.push(Statement::Expression {
                        expression: expr,
                        span,
                    });
                }
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close block")?;
        Ok(Block {
            statements,
            tail_expr,
            span: crate::ast::Span::new(open_span.start, close_span.end),
        })
    }
}
