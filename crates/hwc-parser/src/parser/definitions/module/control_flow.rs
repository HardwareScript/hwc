use crate::ast::{ForLoop, IfConditional, ModuleStatement, Span};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl Parser {
    /// Parse for loop: `for i in 0..63:` or `for i in 0..=63:`
    pub(super) fn parse_for_loop(&mut self) -> Result<ForLoop, ParseError> {
        let start = self.current_span();

        self.expect(&Token::For)?;

        // Parse loop variable
        let variable = self.expect_identifier_string()?;

        self.expect(&Token::In)?;

        // Parse range start
        let range_start = self.expect_integer()?;

        // Check for inclusive or exclusive range
        let inclusive = if self.check(&Token::RangeInclusive) {
            self.advance();
            true
        } else if self.check(&Token::Range) {
            self.advance();
            false
        } else {
            return Err(self.error("Expected '..' or '..=' in for loop range"));
        };

        // Parse range end
        let range_end = self.expect_integer()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.skip_whitespace();
        self.expect(&Token::Indent)?;

        // Parse loop body
        let mut body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check(&Token::Add) {
                body.push(ModuleStatement::AddComponent(self.parse_module_add()?));
            } else if self.check(&Token::Route) {
                body.push(ModuleStatement::Route(self.parse_module_route()?));
            } else if self.check(&Token::For) {
                body.push(ModuleStatement::For(self.parse_for_loop()?));
            } else if self.check(&Token::If) {
                body.push(ModuleStatement::If(self.parse_if_conditional()?));
            } else {
                let current_token = self
                    .current()
                    .map(|t| format!("{}", t.token))
                    .unwrap_or_else(|| "end of input".into());
                return Err(self.error(&format!(
                    "Expected 'add', 'route', 'for', or 'if' in for loop body, found {}",
                    current_token
                )));
            }
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(ForLoop {
            variable: variable.into(),
            start: range_start,
            end: range_end,
            inclusive,
            body,
            span,
        })
    }

    /// Parse if conditional: `if condition:`
    pub(super) fn parse_if_conditional(&mut self) -> Result<IfConditional, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;

        // Parse condition
        let condition = self.parse_condition()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.skip_whitespace();
        self.expect(&Token::Indent)?;

        // Parse then body
        let mut then_body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check(&Token::Add) {
                then_body.push(ModuleStatement::AddComponent(self.parse_module_add()?));
            } else if self.check(&Token::Route) {
                then_body.push(ModuleStatement::Route(self.parse_module_route()?));
            } else if self.check(&Token::For) {
                then_body.push(ModuleStatement::For(self.parse_for_loop()?));
            } else if self.check(&Token::If) {
                then_body.push(ModuleStatement::If(self.parse_if_conditional()?));
            } else {
                let current_token = self
                    .current()
                    .map(|t| format!("{}", t.token))
                    .unwrap_or_else(|| "end of input".into());
                return Err(self.error(&format!(
                    "Expected 'add', 'route', 'for', or 'if' in if body, found {}",
                    current_token
                )));
            }
        }

        self.expect(&Token::Dedent)?;

        // Parse optional else block
        let else_body = if self.check(&Token::Else) {
            self.advance(); // consume 'else'
            self.expect(&Token::Colon)?;
            self.expect(&Token::Newline)?;
            self.skip_whitespace();
            self.expect(&Token::Indent)?;

            let mut else_stmts = Vec::new();
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                if self.check(&Token::Add) {
                    else_stmts.push(ModuleStatement::AddComponent(self.parse_module_add()?));
                } else if self.check(&Token::Route) {
                    else_stmts.push(ModuleStatement::Route(self.parse_module_route()?));
                } else if self.check(&Token::For) {
                    else_stmts.push(ModuleStatement::For(self.parse_for_loop()?));
                } else if self.check(&Token::If) {
                    else_stmts.push(ModuleStatement::If(self.parse_if_conditional()?));
                } else {
                    let current_token = self
                        .current()
                        .map(|t| format!("{}", t.token))
                        .unwrap_or_else(|| "end of input".into());
                    return Err(self.error(&format!(
                        "Expected 'add', 'route', 'for', or 'if' in else body, found {}",
                        current_token
                    )));
                }
            }

            self.expect(&Token::Dedent)?;
            Some(else_stmts)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(IfConditional {
            condition,
            then_body,
            else_body,
            span,
        })
    }
}
