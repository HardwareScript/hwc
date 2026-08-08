use crate::ast::{ArithmeticOp, ArrayIndex, Condition};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};

impl<'ast> Parser<'ast> {
    /// Parse array index expression
    pub(super) fn parse_array_index(&mut self) -> Result<ArrayIndex, ParseError> {
        // Parse the left operand
        let left = if let Some(current) = self.current() {
            if let Token::Identifier(var) = &current.token {
                let var_name = var.clone();
                self.advance();
                ArrayIndex::Variable(var_name)
            } else if let Token::Integer(n) = &current.token {
                // Reject standalone negative integers as array indices
                if *n < 0 {
                    return Err(self.error(
                        "Negative array indices are not supported. Hardware bus and bit indices must be non-negative.",
                    ));
                }
                let value = *n as usize;
                self.advance();
                ArrayIndex::Literal(value)
            } else {
                return Err(self.error(&format!(
                    "Expected variable or number in array index, found {}",
                    current.token
                )));
            }
        } else {
            return Err(self.error("Unexpected end of input in array index"));
        };

        // Check for arithmetic operator OR integer (lexer collision case)
        if self.check(&Token::Plus)
            || self.check(&Token::Hyphen)
            || self.check(&Token::Asterisk)
            || self.check(&Token::Slash)
        {
            let op = if self.check(&Token::Plus) {
                ArithmeticOp::Add
            } else if self.check(&Token::Hyphen) {
                ArithmeticOp::Subtract
            } else if self.check(&Token::Asterisk) {
                ArithmeticOp::Multiply
            } else {
                ArithmeticOp::Divide
            };
            self.advance(); // consume operator

            let right = if let Some(current) = self.current() {
                if let Token::Identifier(var) = &current.token {
                    let var_name = var.clone();
                    self.advance();
                    ArrayIndex::Variable(var_name)
                } else if let Token::Integer(n) = &current.token {
                    let value = *n as usize;
                    self.advance();
                    ArrayIndex::Literal(value)
                } else {
                    return Err(self.error(&format!(
                        "Expected variable or number after arithmetic operator, found {}",
                        current.token
                    )));
                }
            } else {
                return Err(self.error("Unexpected end of input after arithmetic operator"));
            };

            Ok(ArrayIndex::Arithmetic {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        } else if let Some(current) = self.current() {
            // NATIVE FIX: Handle lexer collision where operators get consumed by integer literals
            if let Token::Integer(n) = &current.token {
                if matches!(left, ArrayIndex::Variable(_)) {
                    if *n < 0 {
                        // Convert Integer(-1) into subtraction: i - 1
                        let absolute_value = n.unsigned_abs() as usize;
                        self.advance();
                        return Ok(ArrayIndex::Arithmetic {
                            left: Box::new(left),
                            op: ArithmeticOp::Subtract,
                            right: Box::new(ArrayIndex::Literal(absolute_value)),
                        });
                    } else if *n > 0 {
                        // Convert Integer(1) after variable into addition: i + 1
                        let value = *n as usize;
                        self.advance();
                        return Ok(ArrayIndex::Arithmetic {
                            left: Box::new(left),
                            op: ArithmeticOp::Add,
                            right: Box::new(ArrayIndex::Literal(value)),
                        });
                    }
                }
            }
            Ok(left)
        } else {
            Ok(left)
        }
    }

    /// Parse condition for if statement
    pub(super) fn parse_condition(&mut self) -> Result<Condition, ParseError> {
        // Parse left side
        let left = self.parse_array_index()?;

        // Parse operator
        let condition = if self.check(&Token::Equals) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::Equals { left, right }
        } else if self.check(&Token::LessThan) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::LessThan { left, right }
        } else if self.check(&Token::GreaterThan) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::GreaterThan { left, right }
        } else if self.check(&Token::NotEquals) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::NotEquals { left, right }
        } else {
            let current_token = self
                .current()
                .map(|t| format!("{}", t.token))
                .unwrap_or_else(|| "end of input".into());
            return Err(self.error(&format!(
                "Expected comparison operator (==, <, >, !=), found {}",
                current_token
            )));
        };

        Ok(condition)
    }
}
