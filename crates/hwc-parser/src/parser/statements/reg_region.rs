use crate::ast::{
    ClockEdgeSpec, ClockEdgeType, Expression, RegDecl, RegionDecl, ResetSpec, Span,
};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    pub fn parse_reg_decl(&mut self, start_pos: usize) -> Result<RegDecl, ParseError> {
        self.expect_token(&Token::Reg, "Expected 'reg'")?;
        let name_ident = self.expect_identifier()?;
        let name = CompactString::from(name_ident.name.as_str());

        self.expect_token(&Token::Colon, "Expected ':' after register name")?;
        let type_annotation = self.parse_type_expr()?;

        self.expect_token(&Token::Equals, "Expected '=' for register initial value")?;
        let init_value = self.parse_expression()?;

        let on_start = self.current_span().start;
        if self.check(&Token::On) {
            self.advance();
        } else if let Some(Token::Identifier(s)) = self.current().map(|t| &t.token) {
            if s == "on" {
                self.advance();
            } else {
                return Err(self.error(&format!("Expected 'on:' for clock domain binding, found '{}'", s)));
            }
        } else {
            return Err(self.error("Expected 'on:' for clock domain binding"));
        }

        self.expect_token(&Token::Colon, "Expected ':' after 'on'")?;
        let clock_expr = self.parse_expression()?;

        let (clock, edge) = match clock_expr {
            Expression::FieldAccess { target, field, span } => {
                if field == "posedge" {
                    (*target, ClockEdgeType::Posedge)
                } else if field == "negedge" {
                    (*target, ClockEdgeType::Negedge)
                } else {
                    (
                        Expression::FieldAccess {
                            target,
                            field,
                            span,
                        },
                        ClockEdgeType::Posedge,
                    )
                }
            }
            other => (other, ClockEdgeType::Posedge),
        };

        let clock_edge = ClockEdgeSpec {
            clock,
            edge,
            span: Span::new(on_start, self.previous_span().end),
        };

        let reset = if self.check(&Token::ResetTo)
            || self.current().map(|t| match &t.token {
                Token::Identifier(s) => s == "reset_to",
                _ => false,
            }).unwrap_or(false)
        {
            let reset_start = self.current_span().start;
            self.advance();
            self.expect_token(&Token::Colon, "Expected ':' after 'reset_to'")?;
            let reset_value = self.parse_expression()?;

            if self.check(&Token::When) {
                self.advance();
            } else if let Some(Token::Identifier(s)) = self.current().map(|t| &t.token) {
                if s == "when" {
                    self.advance();
                } else {
                    return Err(self.error(&format!("Expected 'when:' after reset value, found '{}'", s)));
                }
            } else {
                return Err(self.error("Expected 'when:' after reset value"));
            }

            self.expect_token(&Token::Colon, "Expected ':' after 'when'")?;
            let condition = self.parse_expression()?;
            let reset_end = condition.span().end;

            Some(ResetSpec {
                reset_value,
                condition,
                span: Span::new(reset_start, reset_end),
            })
        } else {
            None
        };

        let end_pos = reset.as_ref().map(|r| r.span.end).unwrap_or(clock_edge.span.end);

        Ok(RegDecl {
            name,
            type_annotation,
            init_value,
            clock_edge,
            reset,
            span: Span::new(start_pos, end_pos),
        })
    }

    pub fn parse_region_decl(&mut self, start_pos: usize) -> Result<RegionDecl, ParseError> {
        self.expect_token(&Token::Region, "Expected 'region'")?;
        let name_ident = self.expect_identifier()?;
        let name = CompactString::from(name_ident.name.as_str());

        self.expect_token(&Token::OpenBrace, "Expected '{' for region body")?;
        let mut properties = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            let prop_name = if self.check(&Token::Synthesize) {
                self.advance();
                CompactString::from("synthesize")
            } else {
                let id = self.expect_identifier()?;
                CompactString::from(id.name.as_str())
            };

            self.expect_token(&Token::Colon, "Expected ':' after property name")?;
            let prop_val = self.parse_expression()?;
            properties.push((prop_name, prop_val));

            if self.check(&Token::Comma) || self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close region body")?;
        Ok(RegionDecl {
            name,
            properties,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
