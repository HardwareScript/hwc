//! Lexer implementation with indentation tracking

use logos::Logos;

use super::error::{span_to_source_span, LexError};
use super::span::{Span, SpannedToken};
use super::token::Token;

/// Lexer with indentation tracking
pub struct Lexer<'source> {
    source: &'source str,
    tokens: Vec<SpannedToken>,
    indent_stack: Vec<usize>,
    /// Track bracket depth for implicit line continuation
    open_brackets: usize,
    /// Track if the previous line ended with a comma (for continuation)
    last_token_was_comma: bool,
    /// Track if the last emitted token was a newline (to collapse empty/comment lines)
    last_was_newline: bool,
}

impl<'source> Lexer<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            tokens: Vec::new(),
            indent_stack: vec![0], // Start with 0 indentation
            open_brackets: 0,
            last_token_was_comma: false,
            // Start as true so we automatically ignore leading newlines at the top of the file
            last_was_newline: true,
        }
    }

    /// Tokenize the source code with indentation tracking
    pub fn tokenize(mut self) -> Result<Vec<SpannedToken>, LexError> {
        let mut lexer = Token::lexer(self.source);
        let mut line_start = true;
        let mut _token_count = 0;

        while let Some(token_result) = lexer.next() {
            let span = lexer.span();
            _token_count += 1;

            // if _token_count % 20 == 0 {
            //     eprintln!("[LEXER DEBUG] Processed {} tokens, current position: {}/{}",
            //         _token_count, span.end, self.source.len());
            // }

            match token_result {
                Ok(token) => {
                    match token {
                        Token::Newline => {
                            // If we're inside brackets, skip the newline entirely
                            if self.open_brackets > 0 {
                                continue;
                            }

                            // THE FIX: Collapse multiple sequential newlines (which happen when lines are blank
                            // or contain skipped comments) into a single Newline token.
                            // This saves the parser from expecting an Indent but getting another Newline.
                            if self.last_was_newline {
                                // line_start is already true, just skip emitting another Newline
                                continue;
                            }

                            self.tokens.push(SpannedToken::new(
                                Token::Newline,
                                Span::new(span.start, span.end),
                            ));
                            line_start = true;
                            self.last_was_newline = true;
                        }
                        Token::Comma => {
                            self.last_was_newline = false;
                            self.tokens.push(SpannedToken::new(
                                Token::Comma,
                                Span::new(span.start, span.end),
                            ));
                            self.last_token_was_comma = true;
                        }
                        Token::OpenBracket | Token::OpenParen => {
                            self.last_was_newline = false;
                            self.open_brackets += 1;
                            self.tokens
                                .push(SpannedToken::new(token, Span::new(span.start, span.end)));
                            self.last_token_was_comma = false;
                        }
                        Token::CloseBracket | Token::CloseParen => {
                            self.last_was_newline = false;
                            self.open_brackets = self.open_brackets.saturating_sub(1);
                            self.tokens
                                .push(SpannedToken::new(token, Span::new(span.start, span.end)));
                            self.last_token_was_comma = false;
                        }
                        _ => {
                            self.last_was_newline = false;

                            // Track indentation at start of line (unless inside brackets or after comma)
                            if line_start && self.open_brackets == 0 && !self.last_token_was_comma {
                                // Count spaces before this token
                                let line_text = &self.source[..span.start];
                                if let Some(last_newline) = line_text.rfind('\n') {
                                    // Because skipped comments are on the PREVIOUS line,
                                    // this perfectly extracts only the leading spaces of the current line!
                                    let indent_text = &self.source[last_newline + 1..span.start];
                                    let current_indent = indent_text.len();

                                    // Emit INDENT/DEDENT tokens
                                    self.handle_indentation(current_indent, span.start)?;
                                }
                                line_start = false;
                            } else if line_start {
                                // We're on a new line but inside brackets or after comma - don't track indentation
                                line_start = false;
                            }

                            // Reset comma flag after consuming any non-comma token
                            self.last_token_was_comma = false;

                            self.tokens
                                .push(SpannedToken::new(token, Span::new(span.start, span.end)));
                        }
                    }
                }
                Err(_) => {
                    return Err(LexError::InvalidToken {
                        span: span_to_source_span(&Span::new(span.start, span.end)),
                        text: self.source[span.start..span.end].to_string().into(),
                    });
                }
            }
        }

        // Emit final DEDENTs to return to base indentation
        // eprintln!("[LEXER DEBUG] Lexer loop complete. Total tokens processed: {}, final position: {}/{}",
        //     _token_count, self.source.len(), self.source.len());
        // eprintln!("[LEXER DEBUG] Indent stack depth: {}", self.indent_stack.len());

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.tokens.push(SpannedToken::new(
                Token::Dedent,
                Span::new(self.source.len(), self.source.len()),
            ));
        }

        // Add EOF token
        // eprintln!("[LEXER DEBUG] Adding EOF token. Total tokens in output: {}", self.tokens.len());
        self.tokens.push(SpannedToken::new(
            Token::Eof,
            Span::new(self.source.len(), self.source.len()),
        ));

        Ok(self.tokens)
    }

    fn handle_indentation(&mut self, new_indent: usize, pos: usize) -> Result<(), LexError> {
        let current_indent = *self.indent_stack.last().unwrap();

        if new_indent > current_indent {
            // Indentation increased
            self.indent_stack.push(new_indent);
            self.tokens
                .push(SpannedToken::new(Token::Indent, Span::new(pos, pos)));
        } else if new_indent < current_indent {
            // Indentation decreased - may need multiple DEDENTs
            while let Some(&stack_indent) = self.indent_stack.last() {
                if stack_indent <= new_indent {
                    break;
                }
                self.indent_stack.pop();
                self.tokens
                    .push(SpannedToken::new(Token::Dedent, Span::new(pos, pos)));
            }

            // Check if indentation aligns with a previous level
            if self.indent_stack.last() != Some(&new_indent) {
                return Err(LexError::IndentationError {
                    span: span_to_source_span(&Span::new(pos, pos)),
                    message: format!(
                        "Indentation level {} does not match any previous level",
                        new_indent
                    )
                    .into(),
                });
            }
        }

        Ok(())
    }
}
